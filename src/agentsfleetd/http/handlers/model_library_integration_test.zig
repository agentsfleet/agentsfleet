//! Integration tests for the authenticated model-library read (GET /v1/models).
//!
//! The catalogue ships EMPTY (platform admins populate it via /v1/admin/models).
//! These tests self-seed the rows they assert on through the DB, so they prove
//! the read path without depending on a frozen migration seed. Skips
//! gracefully when TEST_DATABASE_URL is unset.

const std = @import("std");
const clock = @import("common").clock;
const auth_mw = @import("../../auth/middleware/mod.zig");
const scope_fixtures = @import("../test_scope_tokens.zig");

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const model_library_h = @import("model_library.zig");

// Any authenticated persona may read the library; VIEWER (the minimal-scope
// persona) proves the route is authenticated-only, not capability-scoped.
const VIEWER_TOKEN = scope_fixtures.VIEWER;

// uuidv7 literals (version nibble 7) so the library uid CHECK passes.
const UID_SONNET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a8001";
const UID_KIMI = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a8002";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn openHarnessOrSkip(alloc: std.mem.Allocator) !*TestHarness {
    // The fixture JWKS triplet is what lets the offline persona tokens
    // validate against the bearer chain (harness defaults reject them).
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

// Seed the two rows the read-path tests assert on. Rates mirror the retired
// seed (Sonnet $3/M in · $15/M out in nanos) so the wire-shape assertions below
// pin the same values, sourced from a test-owned insert.
fn seedLibrary(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, 'claude-sonnet-4-6', 'anthropic', 256000, 3000000000, 300000000, 15000000000, $3, $3),
        \\       ($2::uuid, 'kimi-k2.6', 'moonshot', 256000, 0, 0, 0, $3, $3)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ UID_SONNET, UID_KIMI, now });
}

fn cleanupLibrary(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.model_library WHERE uid IN ($1::uuid, $2::uuid)", .{ UID_SONNET, UID_KIMI }) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

test "integration(model_library): GET with a valid token returns the catalogue" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seedLibrary(h);
    defer cleanupLibrary(h);

    const r = try (try h.get(model_library_h.MODEL_LIBRARY_PATH).bearer(VIEWER_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"version\""));
    try std.testing.expect(r.bodyContains("claude-sonnet-4-6"));
    try std.testing.expect(r.bodyContains("kimi-k2.6"));
    try std.testing.expect(r.bodyContains("\"context_cap_tokens\":256000"));
    // The row shape is byte-identical to the retired public document's rows:
    // per-token rates accompany every row (zero for self-managed-only models).
    try std.testing.expect(r.bodyContains("\"input_nanos_per_mtok\":3000000000"));
    try std.testing.expect(r.bodyContains("\"cached_input_nanos_per_mtok\":300000000"));
    try std.testing.expect(r.bodyContains("\"output_nanos_per_mtok\":15000000000"));
}

test "integration(model_library): GET without a token returns 401" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try h.get(model_library_h.MODEL_LIBRARY_PATH).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

test "integration(model_library): GET with a garbage token returns 401" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try (try h.get(model_library_h.MODEL_LIBRARY_PATH).bearer("not-a-jwt")).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

test "integration(model_library): empty catalogue returns 200 with models: []" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Deterministic empty state: every suite self-seeds what it asserts on, so
    // clearing the table here cannot break a sibling suite's assertions.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = try conn.exec("DELETE FROM core.model_library", .{});
    }

    const r = try (try h.get(model_library_h.MODEL_LIBRARY_PATH).bearer(VIEWER_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"models\":[]"));
}

test "integration(model_library): POST with a valid token returns 405" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const req = try (try h.post(model_library_h.MODEL_LIBRARY_PATH).bearer(VIEWER_TOKEN)).json("{}");
    const r = try req.send();
    defer r.deinit();
    try r.expectStatus(.method_not_allowed);
}

test "integration(model_library): the retired public cap.json path returns 404" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // pin test: literal is the retired wire path under test — the one allowed
    // in-tree mention of the former route, proving the hard cutover (404, no
    // alias). The zero-references sweep excludes exactly this file.
    const RETIRED_CAP_JSON_PATH = "/_um/da5b6b3810543fe108d816ee972e4ff8/cap.json"; // gitleaks:allow — retired public path obfuscator, never a credential
    const r = try h.get(RETIRED_CAP_JSON_PATH).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}
