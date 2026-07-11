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
const model_library_store = @import("../../state/model_library_store.zig");
const sql = @import("../../state/model_library/sql.zig");

// Any authenticated persona may read the library; VIEWER (the minimal-scope
// persona) proves the route is authenticated-only, not capability-scoped.
const VIEWER_TOKEN = scope_fixtures.VIEWER;

// uuidv7 literals (version nibble 7) so the library uid CHECK passes.
const UID_PRICED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a8001";
const UID_ZERO_RATED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a8002";

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

// Seed the two rows the read-path tests assert on, through the store's own
// create() so the fixture exercises the production insert path. The model ids
// are unique to this suite — several sibling suites seed the shared catalogue
// table with real model ids under their own uids, and a colliding
// (provider, model_id) pair would make create()'s ON CONFLICT silently no-op.
// The affected-count assertions keep any future collision loud.
fn seedLibrary(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    try std.testing.expectEqual(@as(?i64, 1), try model_library_store.create(conn, .{
        .uid = UID_PRICED,
        .provider = "anthropic",
        .model_id = "claude-library-read-fixture",
        .rates = .{ .context_cap_tokens = 256000, .input_nanos_per_mtok = 3000000000, .cached_input_nanos_per_mtok = 300000000, .output_nanos_per_mtok = 15000000000 },
    }, now));
    try std.testing.expectEqual(@as(?i64, 1), try model_library_store.create(conn, .{
        .uid = UID_ZERO_RATED,
        .provider = "moonshot",
        .model_id = "kimi-library-read-fixture",
        .rates = .{ .context_cap_tokens = 256000, .input_nanos_per_mtok = 0, .cached_input_nanos_per_mtok = 0, .output_nanos_per_mtok = 0 },
    }, now));
}

fn cleanupLibrary(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = model_library_store.remove(conn, UID_PRICED) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = model_library_store.remove(conn, UID_ZERO_RATED) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

test "integration(model_library): GET with a valid token returns the catalogue" {
    const alloc = std.testing.allocator;
    const h = try openHarnessOrSkip(alloc);
    defer h.deinit();
    try seedLibrary(h);
    defer cleanupLibrary(h);

    const r = try (try h.get(model_library_h.MODEL_LIBRARY_PATH).bearer(VIEWER_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"version\""));
    try std.testing.expect(r.bodyContains("claude-library-read-fixture"));
    try std.testing.expect(r.bodyContains("kimi-library-read-fixture"));
    try std.testing.expect(r.bodyContains("\"context_cap_tokens\":256000"));
    // The row shape is byte-identical to the retired public document's rows:
    // per-token rates accompany every row (zero for self-managed-only models).
    try std.testing.expect(r.bodyContains("\"input_nanos_per_mtok\":3000000000"));
    try std.testing.expect(r.bodyContains("\"cached_input_nanos_per_mtok\":300000000"));
    try std.testing.expect(r.bodyContains("\"output_nanos_per_mtok\":15000000000"));
}

test "integration(model_library): GET without a token returns 401" {
    const alloc = std.testing.allocator;
    const h = try openHarnessOrSkip(alloc);
    defer h.deinit();

    const r = try h.get(model_library_h.MODEL_LIBRARY_PATH).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

test "integration(model_library): GET with a garbage token returns 401" {
    const alloc = std.testing.allocator;
    const h = try openHarnessOrSkip(alloc);
    defer h.deinit();

    const r = try (try h.get(model_library_h.MODEL_LIBRARY_PATH).bearer("not-a-jwt")).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

test "integration(model_library): empty catalogue returns 200 with models: []" {
    const alloc = std.testing.allocator;
    const h = try openHarnessOrSkip(alloc);
    defer h.deinit();

    // Deterministic empty state: every suite self-seeds what it asserts on, so
    // clearing the table here cannot break a sibling suite's assertions. An
    // active platform default holds a foreign key into this table (NO ACTION),
    // so a populated-defaults database cannot be emptied — skip rather than
    // fail on state this test does not own.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = conn.exec("DELETE FROM " ++ sql.TABLE, .{}) catch |err| {
            std.log.warn("empty-catalogue leg skipped: table not emptiable ({s})", .{@errorName(err)});
            return error.SkipZigTest;
        };
    }

    const r = try (try h.get(model_library_h.MODEL_LIBRARY_PATH).bearer(VIEWER_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"models\":[]"));
}

test "integration(model_library): POST without a token returns 401 (auth runs before the method check)" {
    const alloc = std.testing.allocator;
    const h = try openHarnessOrSkip(alloc);
    defer h.deinit();

    const req = try h.post(model_library_h.MODEL_LIBRARY_PATH).json("{}");
    const r = try req.send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

test "integration(model_library): POST with a valid token returns 405" {
    const alloc = std.testing.allocator;
    const h = try openHarnessOrSkip(alloc);
    defer h.deinit();

    const req = try (try h.post(model_library_h.MODEL_LIBRARY_PATH).bearer(VIEWER_TOKEN)).json("{}");
    const r = try req.send();
    defer r.deinit();
    try r.expectStatus(.method_not_allowed);
}

test "integration(model_library): the retired public cap.json path returns 404" {
    const alloc = std.testing.allocator;
    const h = try openHarnessOrSkip(alloc);
    defer h.deinit();

    // pin test: literal is the retired wire path under test — the one allowed
    // in-tree mention of the former route, proving the hard cutover (404, no
    // alias). The zero-references sweep excludes exactly this file.
    const RETIRED_CAP_JSON_PATH = "/_um/da5b6b3810543fe108d816ee972e4ff8/cap.json"; // gitleaks:allow — retired public path obfuscator, never a credential
    const r = try h.get(RETIRED_CAP_JSON_PATH).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}
