// Paging the tenant model registry, and the delete refusals beside it.
//
// The sibling error-arms suite drives POST and PATCH. What neither it nor the
// happy-path suite ever sent is `starting_after`, so `decodeStart` — the whole
// cursor half of the list endpoint — read dark: the round trip, the two
// distinct rejections, and the tenant-context refusal in front of them.
//
// The two rejections are the reason this matters beyond coverage. A cursor that
// will not decode and a cursor that decodes but names another tenant are
// deliberately different codes, because folding them together would hide a
// cross-tenant replay attempt inside the same signal as a truncated URL. That
// distinction was asserted nowhere.
//
// DB-backed: needs TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");

const base = @import("secrets_json_integration_test.zig");
const ec = @import("../errors/error_registry.zig");
const harness_mod = @import("test_harness.zig");
const scope_fixtures = @import("./test_scope_tokens.zig");

const TestHarness = harness_mod.TestHarness;
const ALLOC = std.testing.allocator;

const MODELS_PATH = "/v1/tenants/me/models";

/// Carries `secret:write` but an empty `metadata` claim, so it clears the
/// route's scope gate and still reaches the handler with
/// `principal.tenant_id == null` — the only way to drive the tenant-context
/// refusal through the real middleware rather than around it.
const TOKEN_NO_TENANT = scope_fixtures.NO_TENANT;

const SECRET_NAME = "paging-key";
const MODEL_A = "claude-sonnet-4-6";
const MODEL_B = "claude-opus-4-1";

const PAGE_OF_ONE = "1";
const PAGE_OF_TWO = "2";

/// Decodes as base64url but is not a cursor this endpoint issued.
const CURSOR_MALFORMED = "bm90LWEtY3Vyc29y";
/// A syntactically valid UUIDv7 naming no entry for this tenant.
const ID_ABSENT = "01960000-0000-7000-8000-00000000beef";
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
        "{{\"name\":\"{s}\",\"data\":{{\"provider\":\"anthropic\",\"api_key\":\"sk-paging\",\"model\":\"{s}\"}}}}",
        .{ SECRET_NAME, MODEL_A },
    );
    defer alloc.free(body);
    const r = try (try (try h.post(path).bearer(base.TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);
}

fn createEntry(h: *TestHarness, alloc: std.mem.Allocator, model_id: []const u8) !void {
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"model_id\":\"{s}\",\"secret_ref\":\"{s}\"}}",
        .{ model_id, SECRET_NAME },
    );
    defer alloc.free(body);
    const r = try (try (try h.post(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);
}

fn listPath(alloc: std.mem.Allocator, limit: []const u8, starting_after: ?[]const u8) ![]const u8 {
    if (starting_after) |cursor| {
        return std.fmt.allocPrint(alloc, "{s}?limit={s}&starting_after={s}", .{ MODELS_PATH, limit, cursor });
    }
    return std.fmt.allocPrint(alloc, "{s}?limit={s}", .{ MODELS_PATH, limit });
}

/// The `next_cursor` of a page, duplicated so it outlives the response. Null
/// when the page is the last one.
fn nextCursor(alloc: std.mem.Allocator, body: []const u8) !?[]const u8 {
    const Page = struct { next_cursor: ?[]const u8 = null };
    const parsed = try std.json.parseFromSlice(Page, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    const cursor = parsed.value.next_cursor orelse return null;
    return try alloc.dupe(u8, cursor);
}

/// The shared seed clears the tenant's secrets and selection but leaves its
/// registry entries standing, so a second test that creates the same model
/// collides with the first test's row. Clearing here keeps each test's page
/// contents its own rather than a function of what ran before it.
fn clearEntries(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    _ = try conn.exec(
        "DELETE FROM core.tenant_model_entries WHERE tenant_id = $1::uuid",
        .{base.TEST_TENANT_ID},
    );
}

fn seedTwoEntries(h: *TestHarness) !void {
    try clearEntries(h);
    try seedSecret(h, ALLOC);
    try createEntry(h, ALLOC, MODEL_A);
    try createEntry(h, ALLOC, MODEL_B);
}

test "integration: the model registry pages through its own cursor" {
    const h = seededHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seedTwoEntries(h);

    const first_path = try listPath(ALLOC, PAGE_OF_ONE, null);
    defer ALLOC.free(first_path);
    const cursor = blk: {
        const r = try (try h.get(first_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        break :blk (try nextCursor(ALLOC, r.body)) orelse return error.TestExpectedCursor;
    };
    defer ALLOC.free(cursor);

    // The second page must resume strictly after the first. Nothing is trusted
    // from the cursor except that boundary — the tenant read is always the
    // authenticated one — so the assertion is that the page moved, not merely
    // that it answered.
    const next_path = try listPath(ALLOC, PAGE_OF_ONE, cursor);
    defer ALLOC.free(next_path);
    const r = try (try h.get(next_path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(!r.bodyContains(cursor));
}

test "integration: the model registry tells a malformed cursor apart from one issued for another page size" {
    const h = seededHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seedTwoEntries(h);

    // Not a cursor this endpoint issued at all.
    {
        const path = try listPath(ALLOC, PAGE_OF_ONE, CURSOR_MALFORMED);
        defer ALLOC.free(path);
        const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_LIBRARY_CURSOR_MALFORMED);
    }

    // A real cursor, replayed against a different page size. It decodes, so the
    // handler has to reject it on identity rather than on shape — the same arm
    // that catches a cursor replayed across tenants, which is why it carries its
    // own code instead of the malformed one.
    const cursor = blk: {
        const path = try listPath(ALLOC, PAGE_OF_ONE, null);
        defer ALLOC.free(path);
        const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        break :blk (try nextCursor(ALLOC, r.body)) orelse return error.TestExpectedCursor;
    };
    defer ALLOC.free(cursor);
    {
        const path = try listPath(ALLOC, PAGE_OF_TWO, cursor);
        defer ALLOC.free(path);
        const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_LIBRARY_CURSOR_MISMATCH);
    }
}

test "integration: listing the model registry refuses a caller carrying no tenant claim" {
    const h = seededHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try listPath(ALLOC, PAGE_OF_ONE, null);
    defer ALLOC.free(path);
    const r = try (try h.get(path).bearer(TOKEN_NO_TENANT)).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
    try r.expectErrorCode(ec.ERR_FORBIDDEN);
}

test "integration: DELETE /models/{id} refuses no tenant and a malformed id, and is idempotent for an absent one" {
    const h = seededHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const absent_path = try std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ MODELS_PATH, ID_ABSENT });
    defer ALLOC.free(absent_path);

    // No tenant claim — refused before the id is even looked at.
    {
        const r = try (try h.delete(absent_path).bearer(TOKEN_NO_TENANT)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }
    // A malformed id is a bad request, distinct from an id that simply names
    // nothing.
    {
        const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ MODELS_PATH, ID_NOT_UUIDV7 });
        defer ALLOC.free(path);
        const r = try (try h.delete(path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_INVALID_REQUEST);
    }
    // Well-formed and absent: 204, matching the secrets verb. The dashboard's
    // retry after a dropped response must not surface as a failure.
    {
        const r = try (try h.delete(absent_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }
}
