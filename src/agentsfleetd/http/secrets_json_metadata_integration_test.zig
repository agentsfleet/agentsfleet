const std = @import("std");
const error_codes = @import("../errors/error_registry.zig");
const base = @import("secrets_json_integration_test.zig");

test "integration: secret endpoints enforce operator role" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(path);
    const body = "{\"name\":\"x\",\"data\":{\"k\":\"v\"}}";
    {
        const r = try (try h.post(path).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    {
        const r = try (try (try h.post(path).bearer(base.TOKEN_USER)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: list projects kind + non-secret metadata, never the api_key" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(creds_path);
    const PROVIDER_KEY_SECRET = "sk-ant-PROVIDER-DO-NOT-LEAK-7f1a";
    const ENDPOINT_SECRET = "sk-compat-DO-NOT-LEAK-9c2b";
    const SECRET_TOKEN = "stripe-DO-NOT-LEAK-3e4d";
    const bodies = [_][]const u8{
        "{\"name\":\"anthropic-prod\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"" ++ PROVIDER_KEY_SECRET ++ "\",\"model\":\"claude-sonnet-4-6\"}}",
        "{\"name\":\"vllm-gw\",\"data\":{\"provider\":\"openai-compatible\",\"base_url\":\"https://gw.example.com/v1\",\"model\":\"kimi-k2.6\",\"api_key\":\"" ++ ENDPOINT_SECRET ++ "\"}}",
        "{\"name\":\"STRIPE_API_KEY\",\"data\":{\"api_token\":\"" ++ SECRET_TOKEN ++ "\"}}",
    };
    for (bodies) |b| {
        const r = try (try (try h.post(creds_path).bearer(base.TOKEN_OPERATOR)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    const r = try (try h.get(creds_path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"kind\":\"provider_key\""));
    try std.testing.expect(r.bodyContains("\"kind\":\"custom_endpoint\""));
    try std.testing.expect(r.bodyContains("\"kind\":\"custom_secret\""));
    try std.testing.expect(r.bodyContains("\"provider\":\"anthropic\""));
    try std.testing.expect(r.bodyContains("\"model\":\"claude-sonnet-4-6\""));
    try std.testing.expect(r.bodyContains("\"provider\":\"openai-compatible\""));
    try std.testing.expect(r.bodyContains("\"base_url\":\"https://gw.example.com/v1\""));
    try std.testing.expect(!r.bodyContains(PROVIDER_KEY_SECRET));
    try std.testing.expect(!r.bodyContains(ENDPOINT_SECRET));
    try std.testing.expect(!r.bodyContains(SECRET_TOKEN));
    try std.testing.expect(!r.bodyContains("api_key"));
    try std.testing.expect(!r.bodyContains("api_token"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: GET list requires operator role" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(creds_path);
    {
        const r = try h.get(creds_path).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    {
        const r = try (try h.get(creds_path).bearer(base.TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: rotate replaces only the api_key, preserving provider/model/base_url" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(creds_path);
    const named_item = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/anthropic-prod", .{base.TEST_WS_ID});
    defer alloc.free(named_item);
    const endpoint_item = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/vllm-gw", .{base.TEST_WS_ID});
    defer alloc.free(endpoint_item);
    const OLD_KEY = "sk-OLD-DO-NOT-LEAK-aa11";
    const NEW_KEY = "sk-NEW-DO-NOT-LEAK-bb22";
    const seed_bodies = [_][]const u8{
        "{\"name\":\"anthropic-prod\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"" ++ OLD_KEY ++ "\",\"model\":\"claude-sonnet-4-6\"}}",
        "{\"name\":\"vllm-gw\",\"data\":{\"provider\":\"openai-compatible\",\"base_url\":\"https://gw.example.com/v1\",\"model\":\"kimi-k2.6\",\"api_key\":\"" ++ OLD_KEY ++ "\"}}",
    };
    for (seed_bodies) |b| {
        const r = try (try (try h.post(creds_path).bearer(base.TOKEN_OPERATOR)).json(b)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    // Replacement is total, so the caller sends every field it wants kept.
    // Nothing is merged — that is what makes the verb expressible on a secret
    // nobody can read back.
    const named_replace = "{\"data\":{\"provider\":\"anthropic\",\"api_key\":\"" ++ NEW_KEY ++ "\",\"model\":\"claude-sonnet-4-6\"}}";
    const endpoint_replace = "{\"data\":{\"provider\":\"openai-compatible\",\"base_url\":\"https://gw.example.com/v1\",\"model\":\"kimi-k2.6\",\"api_key\":\"" ++ NEW_KEY ++ "\"}}";
    {
        const r = try (try (try h.put(named_item).bearer(base.TOKEN_OPERATOR)).json(named_replace)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"name\":\"anthropic-prod\""));
        try std.testing.expect(!r.bodyContains(NEW_KEY));
    }
    {
        const r = try (try (try h.put(endpoint_item).bearer(base.TOKEN_OPERATOR)).json(endpoint_replace)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const r = try (try h.get(creds_path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"provider\":\"anthropic\""));
    try std.testing.expect(r.bodyContains("\"model\":\"claude-sonnet-4-6\""));
    try std.testing.expect(r.bodyContains("\"kind\":\"provider_key\""));
    try std.testing.expect(r.bodyContains("\"base_url\":\"https://gw.example.com/v1\""));
    try std.testing.expect(r.bodyContains("\"model\":\"kimi-k2.6\""));
    try std.testing.expect(!r.bodyContains(OLD_KEY));
    try std.testing.expect(!r.bodyContains(NEW_KEY));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: rotate a missing secret returns typed 404" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/does-not-exist", .{base.TEST_WS_ID});
    defer alloc.free(path);
    const r = try (try (try h.put(path).bearer(base.TOKEN_OPERATOR)).json("{\"data\":{\"api_key\":\"sk-whatever\"}}")).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
    try std.testing.expect(r.bodyContains(error_codes.ERR_SECRET_NOT_FOUND));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: replace rejects an empty or oversized body without leaking it" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(creds_path);
    const item_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/anthropic-prod", .{base.TEST_WS_ID});
    defer alloc.free(item_path);
    {
        const r = try (try (try h.post(creds_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"anthropic-prod\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-seed\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
    {
        // An empty object is rejected by the same gate `create` uses, so the
        // two verbs cannot disagree about what a secret is.
        const r = try (try (try h.put(item_path).bearer(base.TOKEN_OPERATOR)).json("{\"data\":{}}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_VAULT_DATA_INVALID));
    }
    {
        const filler = try alloc.alloc(u8, 5 * 1024);
        defer alloc.free(filler);
        @memset(filler, 'k');
        const body = try std.fmt.allocPrint(alloc, "{{\"data\":{{\"api_key\":\"{s}\"}}}}", .{filler});
        defer alloc.free(body);
        const r = try (try (try h.put(item_path).bearer(base.TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try std.testing.expect(r.bodyContains(error_codes.ERR_VAULT_DATA_TOO_LARGE));
        try std.testing.expect(!r.bodyContains(filler));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: cross-workspace DELETE is rejected (IDOR guard)" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const other_ws = "0195b4ba-8d3a-7f13-8abc-deadbeef0001";
    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/fly", .{other_ws});
    defer alloc.free(path);
    const r = try (try h.delete(path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try std.testing.expect(r.status >= 400);
    try std.testing.expect(r.status != 204);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: test_put_replaces_whole_body — a dropped field is gone, not retained" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const creds_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(creds_path);
    const item_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets/shape-shifter", .{base.TEST_WS_ID});
    defer alloc.free(item_path);

    // Seed a provider-shaped secret: the list projects `provider` for it.
    {
        const r = try (try (try h.post(creds_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"shape-shifter\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-seed\",\"model\":\"claude-sonnet-4-6\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    // Replace it with a body that shares no field at all. Under the former
    // field-merge this was impossible to express; under a whole-body replace
    // it is the ordinary case, and every old field must vanish.
    {
        const r = try (try (try h.put(item_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"data\":{\"token\":\"ghp-replaced\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    // The projection is derived from the stored bytes, so the list is the
    // observable proof: a secret that was a provider key is now an opaque
    // custom secret, and `anthropic` is gone rather than merged around.
    const r = try (try h.get(creds_path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"name\":\"shape-shifter\""));
    try std.testing.expect(!r.bodyContains("\"provider\":\"anthropic\""));
    try std.testing.expect(!r.bodyContains("ghp-replaced"));

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}
