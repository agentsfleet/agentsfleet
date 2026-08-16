// The refusal half of POST /v1/tenants/me/models and the whole of
// PATCH /v1/tenants/me/models/{id}.
//
// The sibling suite drives the create guards and the list projections. What was
// dark is every arm the handler takes when the caller is wrong — no tenant
// claim, an absent or non-JSON body, an empty model_id, an empty secret_ref, an
// id that is not a UUIDv7, an id that does not resolve for this tenant — plus
// the PATCH verb entirely, which had no coverage at all and so could have
// stopped resolving ids without a single test noticing.
//
// Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise via
// `TestHarness.start` returning `error.SkipZigTest`.

const std = @import("std");

const base = @import("secrets_json_integration_test.zig");
const scope_fixtures = @import("./test_scope_tokens.zig");
const harness_mod = @import("test_harness.zig");
const ec = @import("../errors/error_registry.zig");

const TestHarness = harness_mod.TestHarness;

const MODELS_PATH = "/v1/tenants/me/models";

/// Carries `secret:write` but an empty `metadata` claim, so it clears the
/// route's scope gate and still reaches the handler with
/// `principal.tenant_id == null` — the only way to drive the tenant-context
/// refusal through the real middleware rather than around it.
const TOKEN_NO_TENANT = scope_fixtures.NO_TENANT;

const SECRET_NAME = "error-arms-key";
const MODEL_A = "claude-sonnet-4-6";
const MODEL_B = "claude-opus-4-1";
const MODEL_C = "claude-haiku-4-5";

// A body sent as zero-length rather than as a bodiless request: std's client
// asserts `!method.requestHasBody()` in `sendBodilessUnflushed`, so a bodiless
// POST or PATCH panics inside the client and never reaches the server.
const BODY_EMPTY = "";
const BODY_NOT_JSON = "not-json-at-all";
const BODY_MODEL_ID_BLANK = "{\"model_id\":\"\"}";

// A syntactically valid UUIDv7 that names no row for this tenant.
const ID_ABSENT = "01960000-0000-7000-8000-00000000dead";
const ID_NOT_UUIDV7 = "not-a-uuid";

fn seededHarness(alloc: std.mem.Allocator) !*TestHarness {
    base.setTestEncryptionKey();
    return base.seedAndHarness(alloc);
}

fn seedSecret(h: *TestHarness, alloc: std.mem.Allocator) !void {
    const path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(path);
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"{s}\",\"data\":{{\"provider\":\"anthropic\",\"api_key\":\"sk-error-arms\",\"model\":\"{s}\"}}}}",
        .{ SECRET_NAME, MODEL_A },
    );
    defer alloc.free(body);
    const r = try (try (try h.post(path).bearer(base.TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);
}

/// POST one entry against the seeded secret and hand back its id. Caller owns it.
fn createEntry(h: *TestHarness, alloc: std.mem.Allocator, model_id: []const u8) ![]const u8 {
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"model_id\":\"{s}\",\"secret_ref\":\"{s}\"}}",
        .{ model_id, SECRET_NAME },
    );
    defer alloc.free(body);
    const r = try (try (try h.post(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    const IdOnly = struct { id: []const u8 };
    const parsed = try std.json.parseFromSlice(IdOnly, alloc, r.body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    return alloc.dupe(u8, parsed.value.id);
}

fn entryPath(alloc: std.mem.Allocator, id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}", .{ MODELS_PATH, id });
}

fn patchModel(h: *TestHarness, alloc: std.mem.Allocator, id: []const u8, model_id: []const u8) !harness_mod.Response {
    const path = try entryPath(alloc, id);
    defer alloc.free(path);
    const body = try std.fmt.allocPrint(alloc, "{{\"model_id\":\"{s}\"}}", .{model_id});
    defer alloc.free(body);
    return (try (try h.patch(path).bearer(base.TOKEN_OPERATOR)).json(body)).send();
}

test "integration: POST and PATCH /models refuse a caller carrying no tenant claim" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Both verbs read the tenant claim before touching the pool, so both must
    // refuse identically rather than 500 on a null tenant.
    {
        const r = try (try (try h.post(MODELS_PATH).bearer(TOKEN_NO_TENANT))
            .json("{\"model_id\":\"" ++ MODEL_A ++ "\",\"secret_ref\":\"" ++ SECRET_NAME ++ "\"}")).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }
    {
        const path = try entryPath(alloc, ID_ABSENT);
        defer alloc.free(path);
        const r = try (try (try h.patch(path).bearer(TOKEN_NO_TENANT))
            .json("{\"model_id\":\"" ++ MODEL_A ++ "\"}")).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: POST /models refuses an absent body, a non-JSON body, a blank model_id and a blank secret_ref" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Each of these must be refused before the pool is touched — the entry
    // registry never opens a transaction to learn the caller sent nothing.
    const bodies = [_][]const u8{
        BODY_EMPTY,
        BODY_NOT_JSON,
        "{\"model_id\":\"\",\"secret_ref\":\"" ++ SECRET_NAME ++ "\"}",
        "{\"model_id\":\"" ++ MODEL_A ++ "\",\"secret_ref\":\"\"}",
    };
    for (bodies) |body| {
        const r = try (try (try h.post(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_INVALID_REQUEST);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: PATCH /models/{id} refuses a malformed id, an absent body, a non-JSON body and a blank model_id" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // The id is checked before the body, so a malformed id refuses whatever the
    // body carries.
    {
        const path = try entryPath(alloc, ID_NOT_UUIDV7);
        defer alloc.free(path);
        const r = try (try (try h.patch(path).bearer(base.TOKEN_OPERATOR))
            .json("{\"model_id\":\"" ++ MODEL_A ++ "\"}")).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_INVALID_REQUEST);
    }

    const path = try entryPath(alloc, ID_ABSENT);
    defer alloc.free(path);
    const bodies = [_][]const u8{ BODY_EMPTY, BODY_NOT_JSON, BODY_MODEL_ID_BLANK };
    for (bodies) |body| {
        const r = try (try (try h.patch(path).bearer(base.TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        // A well-formed id that names no row would 404, so a 400 here proves the
        // body was rejected first rather than the lookup having run.
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_INVALID_REQUEST);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: PATCH /models/{id} 404s an id that does not resolve for this tenant" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try patchModel(h, alloc, ID_ABSENT, MODEL_B);
    defer r.deinit();
    try r.expectStatus(.not_found);
    try r.expectErrorCode(ec.ERR_MODELS_ENTRY_NOT_FOUND);

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: PATCH /models/{id} changes the model, and refuses a change that collides with a sibling" {
    const alloc = std.testing.allocator;
    const h = seededHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    try seedSecret(h, alloc);

    const first = try createEntry(h, alloc, MODEL_A);
    defer alloc.free(first);
    const second = try createEntry(h, alloc, MODEL_B);
    defer alloc.free(second);

    // Both entries name the same secret, so moving the second onto the first's
    // model makes the pair identical — the uniqueness constraint the state
    // layer translates into DuplicateEntry.
    {
        const r = try patchModel(h, alloc, second, MODEL_A);
        defer r.deinit();
        try r.expectStatus(.conflict);
        try r.expectErrorCode(ec.ERR_MODELS_DUPLICATE_ENTRY);
    }

    // A model nothing else holds is accepted, and the response carries the new
    // model against the unchanged id and secret_ref — secret_ref being immutable
    // on this verb is the point of the assertion.
    {
        const r = try patchModel(h, alloc, second, MODEL_C);
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains(MODEL_C));
        try std.testing.expect(r.bodyContains(second));
        try std.testing.expect(r.bodyContains(SECRET_NAME));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}
