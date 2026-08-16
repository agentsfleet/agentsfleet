// The refusal half of PUT /v1/admin/platform-keys and the whole of
// DELETE /v1/admin/platform-keys/{provider}.
//
// The sibling suite (model_library_admin_integration_test.zig) drives the PUT
// success paths, single-active invariant and catalogue guard. What was dark is
// every arm the handler takes when the admin sends something wrong — an absent
// or non-JSON body, a provider or model outside its length bound, a base_url
// paired with a named provider, a source_workspace_id naming no workspace — and
// the DELETE verb entirely, which stands a provider down and NULLs its model so
// the catalogue row it pinned becomes deletable.
//
// Self-contained rows: this suite seeds and deletes its own tenant, workspace
// and catalogue entries under ids no sibling touches, because the shared-fixture
// suites clean up each other's rows and a stood-down default is exactly the
// state a neighbour's teardown would erase mid-test.
//
// DB-backed: needs TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const clock = @import("common").clock;

const scope_fixtures = @import("../../test_scope_tokens.zig");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const error_registry = @import("../../../errors/error_registry.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;
const ALLOC = std.testing.allocator;

const PLATFORM_KEYS_PATH = "/v1/admin/platform-keys";
const PLATFORM_ADMIN_TOKEN = scope_fixtures.PLATFORM_ADMIN;

const TENANT_ID = "0196a000-0000-7000-8000-00000000e001";
const WORKSPACE_ID = "0196a000-0000-7000-8000-00000000e002";
const MODEL_UID = "0196a000-0000-7000-8000-00000000e003";
/// A syntactically valid UUIDv7 naming no workspace row.
const WORKSPACE_ID_ABSENT = "0196a000-0000-7000-8000-00000000e0ff";

const PROVIDER = "m164ea";
const MODEL_ID = "error-arms-1";

// 33 characters — one past the handler's 32-char provider bound, which is the
// only way to reach the DELETE length refusal: an empty provider produces a
// path the router never matches to this route at all.
const PROVIDER_TOO_LONG = "m164eaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
// 257 characters — one past the model bound.
const MODEL_TOO_LONG = "m" ** 257;

const BODY_EMPTY = "";
const BODY_NOT_JSON = "not-json-at-all";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seed(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    _ = try conn.exec(
        "INSERT INTO core.tenants (id, name, created_at, updated_at) VALUES ($1::uuid, 'Platform Key Error Arms', $2, $2) ON CONFLICT (id) DO NOTHING",
        .{ TENANT_ID, now },
    );
    _ = try conn.exec(
        "INSERT INTO core.workspaces (id, tenant_id, name, created_at) VALUES ($1::uuid, $2::uuid, 'platform-key-error-arms', $3) ON CONFLICT (id) DO NOTHING",
        .{ WORKSPACE_ID, TENANT_ID, now },
    );
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 128000, 1, 0, 2, $4, $4)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ MODEL_UID, MODEL_ID, PROVIDER, now });
}

/// Explicit, never deferred at the suite level: deferred cleanup leaks pool
/// connections at `pool.deinit()`.
fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.platform_provider_defaults WHERE source_workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.model_library WHERE provider = $1", .{PROVIDER}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.workspaces WHERE id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenants WHERE id = $1::uuid", .{TENANT_ID}) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn putBody(alloc: std.mem.Allocator, provider: []const u8, workspace_id: []const u8, model: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"provider\":\"{s}\",\"source_workspace_id\":\"{s}\",\"model\":\"{s}\"}}",
        .{ provider, workspace_id, model },
    );
}

fn expectPutRejected(h: *TestHarness, body: []const u8, code: []const u8) !void {
    const r = try (try (try h.put(PLATFORM_KEYS_PATH).bearer(PLATFORM_ADMIN_TOKEN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(code);
}

test "platform keys: PUT refuses an absent body and a body that is not JSON" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Sent zero-length rather than bodiless: std's client asserts
    // `!method.requestHasBody()` in `sendBodilessUnflushed`, so a bodiless PUT
    // panics in the client and never reaches the server.
    try expectPutRejected(h, BODY_EMPTY, error_registry.ERR_INVALID_REQUEST);
    try expectPutRejected(h, BODY_NOT_JSON, error_registry.ERR_INVALID_REQUEST);

    cleanup(h);
}

test "platform keys: PUT refuses a provider or model outside its length bound" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);

    // Both bounds are checked before the pool is touched, so neither refusal
    // depends on the workspace or the catalogue existing.
    {
        const body = try putBody(ALLOC, "", WORKSPACE_ID, MODEL_ID);
        defer ALLOC.free(body);
        try expectPutRejected(h, body, error_registry.ERR_INVALID_REQUEST);
    }
    {
        const body = try putBody(ALLOC, PROVIDER_TOO_LONG, WORKSPACE_ID, MODEL_ID);
        defer ALLOC.free(body);
        try expectPutRejected(h, body, error_registry.ERR_INVALID_REQUEST);
    }
    {
        const body = try putBody(ALLOC, PROVIDER, WORKSPACE_ID, "");
        defer ALLOC.free(body);
        try expectPutRejected(h, body, error_registry.ERR_INVALID_REQUEST);
    }
    {
        const body = try putBody(ALLOC, PROVIDER, WORKSPACE_ID, MODEL_TOO_LONG);
        defer ALLOC.free(body);
        try expectPutRejected(h, body, error_registry.ERR_INVALID_REQUEST);
    }

    cleanup(h);
}

test "platform keys: PUT refuses a base_url smuggled onto a named provider" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);

    // A named provider carrying a base_url would silently widen the egress
    // allowlist without going through the openai-compatible path, so the pairing
    // rule refuses it before any row is written.
    const body = "{\"provider\":\"" ++ PROVIDER ++ "\",\"source_workspace_id\":\"" ++ WORKSPACE_ID ++
        "\",\"model\":\"" ++ MODEL_ID ++ "\",\"base_url\":\"https://example.invalid/v1\"}";
    try expectPutRejected(h, body, error_registry.ERR_PROVIDER_BASE_URL_INVALID);

    cleanup(h);
}

test "platform keys: PUT refuses a source_workspace_id that names no workspace" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);

    // Well-formed UUIDv7, so the id-shape gate passes and the existence probe is
    // what refuses — the arm that separates "malformed id" from "no such row".
    const body = try putBody(ALLOC, PROVIDER, WORKSPACE_ID_ABSENT, MODEL_ID);
    defer ALLOC.free(body);
    try expectPutRejected(h, body, error_registry.ERR_INVALID_REQUEST);

    cleanup(h);
}

test "platform keys: DELETE refuses a provider past the length bound" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ PLATFORM_KEYS_PATH, PROVIDER_TOO_LONG });
    defer ALLOC.free(path);
    const r = try (try h.delete(path).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode(error_registry.ERR_INVALID_REQUEST);

    cleanup(h);
}

test "platform keys: DELETE stands the active provider down and the list stops reporting it active" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);

    {
        const body = try putBody(ALLOC, PROVIDER, WORKSPACE_ID, MODEL_ID);
        defer ALLOC.free(body);
        const r = try (try (try h.put(PLATFORM_KEYS_PATH).bearer(PLATFORM_ADMIN_TOKEN)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ PLATFORM_KEYS_PATH, PROVIDER });
    defer ALLOC.free(path);
    {
        const r = try (try h.delete(path).bearer(PLATFORM_ADMIN_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains(PROVIDER));
    }

    // Deactivating twice is not an error — the statement matches no active row
    // the second time and the verb still answers, which is what makes the admin
    // page's retry safe.
    {
        const r = try (try h.delete(path).bearer(PLATFORM_ADMIN_TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    cleanup(h);
}
