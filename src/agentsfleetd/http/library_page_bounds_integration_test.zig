//! Integration tier for §3 Dimension 3.2 — the three rows of the resource table
//! that `library_read_bounds_integration_test.zig` could not assert.
//!
//! That file measures the tenant registry. These are the global models page and
//! the two Fleet reads, which did not exist when it was written: asserting a
//! budget for a route that 404s passes for the wrong reason, and keeps passing
//! once the handler lands. They exist now, so the rows are real.
//!
//! Separate file rather than more of the same one, because the same one is at
//! its 350-line cap. The seam is the resource: registry there, pages here.
//!
//! ## Two of these numbers were drafted wrong, and the measurement says so
//!
//! §3 budgeted the Fleet summary at ≤1 statement and the Fleet detail at ≤2.
//! Both omit `common.authorizeWorkspace`, which costs TWO — one resolving the
//! principal's tenant through `core.users`, one checking the workspace belongs
//! to it — and which runs inside the measured window.
//!
//! That is not an accounting slip in the handler; it is where the boundary
//! actually falls. §3 states the window as "after middleware auth", and
//! workspace authorization is NOT middleware: the bearer chain authenticates,
//! and the handler authorizes, because only the handler knows which workspace
//! the path names. `beginRead()` opens at handler entry, so those two statements
//! are inside the budget by construction. The table is corrected to the
//! measurement, exactly as the tenant registry row was corrected twice.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;

const auth_mw = @import("../auth/middleware/mod.zig");
const scope_fixtures = @import("test_scope_tokens.zig");
const http_auth = @import("../db/test_fixtures_http_auth.zig");
const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const counters = @import("../observability/library_read_counters.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const model_library_store = @import("../state/model_library_store.zig");
const model_library_cache = @import("../state/model_library_cache.zig");
const library_store = @import("../fleet_library/library_store.zig");

const TOKEN = scope_fixtures.TENANT_ADMIN;
const MODELS_PATH = "/v1/models";

const UID_ONE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ac001";
const UID_TWO = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ac002";
const CATALOGUE_MODEL = "m143bounds-model";
const CATALOGUE_VENDOR_A = "m143bounds-vendor-a";
const CATALOGUE_VENDOR_B = "m143bounds-vendor-b";

const FLEET_ONE = "m143bounds-fleet-1";
const FLEET_TWO = "m143bounds-fleet-2";
const CONTENT_HASH = "0000000000000000000000000000000000000000000000000000000000000243";

// Both Fleet rows are asserted against `counters.FLEET_*_MAX_STATEMENTS`
// directly, not against arithmetic spelled here. The first draft of this file
// did the arithmetic — authorization pair plus the table's own
// `FLEET_DETAIL_MAX_STATEMENTS` — and expected 4 against a measured 3, because
// the table's ≤2 had already folded in an authorization cost of one. A number
// carried out of a table and re-added in a test is not a measurement; it is the
// same guess twice. The constants now hold the measurement and this file
// compares against them, which is what that module exists for.

const RATES: model_library_store.Rates = .{
    .context_cap_tokens = 64000,
    .input_nanos_per_mtok = 10,
    .cached_input_nanos_per_mtok = 1,
    .output_nanos_per_mtok = 20,
};

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn openOrSkip(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

/// Assertions every row of §3's table shares, so a row cannot quietly enforce a
/// weaker set than its neighbour.
fn expectCommon(measured: counters.Snapshot, body_len: usize) !void {
    // No library read decrypts (Invariant 5) — asserted on every path, not only
    // the one that used to.
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());
    try std.testing.expectEqual(counters.MAX_CONNECTIONS_PER_READ, measured.connections);
    // The tally describes the body the client actually received. Without this
    // the handler's own measurement is self-certifying.
    try std.testing.expectEqual(body_len, measured.encoded_bytes);
    try std.testing.expect(measured.encoded_bytes > 0);
}

/// Idempotent: removes this suite's uids before inserting them.
///
/// `INSERT_ROW` carries `ON CONFLICT (provider, model_id) DO NOTHING`, which
/// does NOT cover the `uid` primary key — a leftover row under the same uid is a
/// hard constraint violation, not a silent no-op. A prior run that died between
/// seeding and cleanup therefore poisons every later run, and the failure points
/// at the seed rather than at whatever actually broke. Deleting first makes the
/// suite's starting state its own business.
fn seedCatalogue(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    for ([_][]const u8{ UID_ONE, UID_TWO }) |uid| {
        _ = try model_library_store.remove(conn, uid);
    }
    const now = clock.nowMillis();
    _ = try model_library_store.create(conn, .{
        .uid = UID_ONE,
        .provider = CATALOGUE_VENDOR_A,
        .model_id = CATALOGUE_MODEL,
        .rates = RATES,
    }, now);
    _ = try model_library_store.create(conn, .{
        .uid = UID_TWO,
        .provider = CATALOGUE_VENDOR_B,
        .model_id = CATALOGUE_MODEL,
        .rates = RATES,
    }, now);
}

fn cleanCatalogue(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    for ([_][]const u8{ UID_ONE, UID_TWO }) |uid| {
        _ = model_library_store.remove(conn, uid) catch |err|
            std.log.warn("catalogue cleanup ignored: {s}", .{@errorName(err)});
    }
}

test "integration: test_library_read_resource_bounds — the global models page costs two statements on a miss and one on a hit" {
    const alloc = std.testing.allocator;
    const h = openOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    // Registered BEFORE the seed, not after: a `defer` placed after a fallible
    // call never runs when that call is the thing that fails, so a half-seeded
    // catalogue would survive the test and break the next run at its insert.
    defer cleanCatalogue(h);
    try seedCatalogue(h);

    // The harness leaves `Context.model_library_cache` null, so this is the only
    // place the HIT row can be measured at all. Set on `&h.ctx` before the
    // request, per the harness's Option-C convention for boot-resolved fields.
    var cache = try model_library_cache.Cache.init(alloc);
    defer cache.deinit();
    h.ctx.model_library_cache = &cache;
    // Cleared before the harness tears down: the Context outlives this scope's
    // `cache`, and a dangling pointer there would fail a LATER test.
    defer h.ctx.model_library_cache = null;

    const path = MODELS_PATH ++ "?q=" ++ CATALOGUE_MODEL ++ "&limit=100";

    // ── miss: the revision read, then the page ───────────────────────────────
    {
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        const measured = counters.snapshot();
        try std.testing.expectEqual(counters.GLOBAL_MODELS_MAX_STATEMENTS_MISS, measured.statements);
        try expectCommon(measured, r.body.len);
        try std.testing.expect(measured.results <= counters.TENANT_REGISTRY_MAX_RESULTS);
        try std.testing.expect(measured.encoded_bytes <= counters.GLOBAL_MODELS_MAX_BODY_BYTES);
    }

    // ── hit: the revision read ALONE ─────────────────────────────────────────
    //
    // The revision is read before cache selection on every request, deliberately
    // — that ordering is what makes a stale candidate unreachable rather than
    // dangerous. So a hit is one statement, never zero, and asserting zero here
    // would be asserting that the generation check had been skipped.
    {
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        const measured = counters.snapshot();
        try std.testing.expectEqual(counters.GLOBAL_MODELS_MAX_STATEMENTS_HIT, measured.statements);
        try expectCommon(measured, r.body.len);
    }
}

fn seedFleets(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    _ = try conn.exec("DELETE FROM core.tenant_fleet_library", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
    for ([_][]const u8{ FLEET_ONE, FLEET_TWO }) |id| {
        _ = try conn.exec(
            \\INSERT INTO core.fleet_library
            \\  (id, name, description, source_repo, source_path, source_ref,
            \\   required_credentials, required_credentials_reasons, required_tools,
            \\   network_hosts, visibility, content_hash, created_at, updated_at)
            \\VALUES ($1, $1, 'bounds fixture', $1, '', 'main',
            \\        '[]'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, $2, $3, 1, 1)
        , .{ id, library_store.VISIBILITY_PUBLIC, CONTENT_HASH });
    }
}

fn galleryUrl(alloc: std.mem.Allocator, extra: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleet-libraries{s}", .{ http_auth.WS_PRIMARY, extra });
}

test "integration: test_library_read_resource_bounds — both Fleet reads pay for workspace authorization and nothing else" {
    const alloc = std.testing.allocator;
    const h = openOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedFleets(conn);
    }

    // ── the summary: authorization, then ONE merged read across both tables ──
    {
        const url = try galleryUrl(alloc, "?limit=100");
        defer alloc.free(url);
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(url).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        const measured = counters.snapshot();
        // ONE statement for the page itself, whatever `limit` is and whichever
        // tables hold the rows — the UNION is what buys that, and it is the
        // number §3 actually cares about. The authorization pair is fixed
        // overhead that does not scale with the page.
        try std.testing.expectEqual(
            counters.FLEET_SUMMARY_MAX_STATEMENTS,
            measured.statements,
        );
        try expectCommon(measured, r.body.len);
        try std.testing.expect(measured.encoded_bytes <= counters.FLEET_SUMMARY_MAX_BODY_BYTES);
        // Bounded by `limit`, not by how many rows happen to exist.
        try std.testing.expect(measured.results <= counters.TENANT_REGISTRY_MAX_RESULTS);
    }

    // ── the detail: authorization, then ONE single-entry read ────────────────
    {
        const url = try galleryUrl(alloc, "/platform/" ++ FLEET_ONE);
        defer alloc.free(url);
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(url).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        const measured = counters.snapshot();
        try std.testing.expectEqual(
            counters.FLEET_DETAIL_MAX_STATEMENTS,
            measured.statements,
        );
        try expectCommon(measured, r.body.len);
        // Exactly one row. A detail route that returned two would still satisfy
        // every ceiling above.
        try std.testing.expectEqual(counters.FLEET_DETAIL_MAX_RESULTS, measured.results);
        try std.testing.expect(measured.encoded_bytes <= counters.FLEET_DETAIL_MAX_BODY_BYTES);
    }
}
