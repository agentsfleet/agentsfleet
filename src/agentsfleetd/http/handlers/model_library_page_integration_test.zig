//! Integration tier for §2 Dimension 2.1 — `test_model_page_and_conditional_headers`.
//!
//! Two halves, both about `GET /v1/models`: the normalized keyset (order,
//! resume, filters, and every 400) and the conditional read (`ETag`,
//! `Cache-Control`, `Vary`, and 200-vs-304).
//!
//! ## The fixtures tie on purpose
//!
//! §2 sorts by normalized display, then normalized vendor, then uid — three
//! keys, each `COLLATE "C"`. The second and third are UNREACHABLE until the one
//! before them ties, so a fixture set with four distinct `model_id`s would pass
//! against a single-key sort and against a reversed vendor tiebreak alike. Two
//! rows here therefore share a `model_id` and differ only by provider, which is
//! the only shape where the vendor comparison decides anything.
//!
//! ## Why the cache is not wired here
//!
//! `Context.model_library_cache` is null under the harness, so every request
//! below is a cache MISS. That is the right tier for this dimension: the 304 is
//! computed from the body's own ETag, not from cache residency, so a conditional
//! read must work identically with and without one. Dimension 2.2 owns the cache
//! and is tested against it directly.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");
const clock = @import("common").clock;

const auth_mw = @import("../../auth/middleware/mod.zig");
const scope_fixtures = @import("../test_scope_tokens.zig");
const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const model_library_store = @import("../../state/model_library_store.zig");
const etag = @import("../etag.zig");
const ec = @import("../../errors/error_registry.zig");
const pagination = @import("../pagination.zig");
const catalogue_key = @import("library/catalogue_key.zig");

const MODELS_PATH = "/v1/models";
const VIEWER = scope_fixtures.VIEWER;

/// uuidv7 literals (version nibble 7) so the library uid CHECK passes. Ordered
/// ascending so the uid tiebreak is predictable where it is reached.
const UID_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9001";
const UID_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9002";
const UID_C = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9003";
const UID_D = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a9004";

/// Suite-unique ids. Sibling suites seed the shared catalogue table under their
/// own uids, and this suite asserts on ORDER, so a colliding identifier would
/// not merely add a row — it would move one.
const MODEL_ALPHA = "m143page-alpha";
const MODEL_MID = "m143page-mid";
const MODEL_ZETA = "m143page-zeta";
const VENDOR_ALPHA = "m143page-vendor-a";
const VENDOR_BETA = "m143page-vendor-b";
const VENDOR_GAMMA = "m143page-vendor-g";

/// The whole suite's search needle: every seeded row matches it and no sibling
/// suite's row does, so `q` is what isolates this fixture set from a shared
/// table rather than a cleanup that would race other suites.
const SUITE_NEEDLE = "m143page";

/// Filler rates. This suite asserts on ORDER and on headers, never on a price,
/// so the values carry no contract — they are named only because a bare literal
/// in a fixture reads like one (UFS). Non-zero and mutually distinct so that a
/// future assertion about a projected rate cannot pass against a page that
/// carried none.
const FIXTURE_CONTEXT_CAP: u32 = 128_000;
const FIXTURE_INPUT_NANOS: i64 = 1_000;
const FIXTURE_CACHED_NANOS: i64 = 100;
const FIXTURE_OUTPUT_NANOS: i64 = 2_000;

const RATES: model_library_store.Rates = .{
    .context_cap_tokens = FIXTURE_CONTEXT_CAP,
    .input_nanos_per_mtok = FIXTURE_INPUT_NANOS,
    .cached_input_nanos_per_mtok = FIXTURE_CACHED_NANOS,
    .output_nanos_per_mtok = FIXTURE_OUTPUT_NANOS,
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

/// Four rows whose normalized sort order is known and whose first key TIES.
///
/// Expected order — display, then vendor, then uid:
///   1. m143page-alpha / vendor-a   (display ties with 2; vendor decides)
///   2. m143page-alpha / vendor-b
///   3. m143page-mid   / vendor-g
///   4. m143page-zeta  / vendor-a
fn seed(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    // Inserted out of sort order so a passing assertion cannot be insertion
    // order wearing the ORDER BY's clothes.
    try expectInserted(model_library_store.create(conn, .{
        .uid = UID_D,
        .provider = VENDOR_ALPHA,
        .model_id = MODEL_ZETA,
        .rates = RATES,
    }, now));
    try expectInserted(model_library_store.create(conn, .{
        .uid = UID_B,
        .provider = VENDOR_BETA,
        .model_id = MODEL_ALPHA,
        .rates = RATES,
    }, now));
    try expectInserted(model_library_store.create(conn, .{
        .uid = UID_A,
        .provider = VENDOR_ALPHA,
        .model_id = MODEL_ALPHA,
        .rates = RATES,
    }, now));
    try expectInserted(model_library_store.create(conn, .{
        .uid = UID_C,
        .provider = VENDOR_GAMMA,
        .model_id = MODEL_MID,
        .rates = RATES,
    }, now));
}

fn expectInserted(res: anyerror!?i64) !void {
    try std.testing.expectEqual(@as(?i64, 1), try res);
}

fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    for ([_][]const u8{ UID_A, UID_B, UID_C, UID_D }) |uid| {
        _ = model_library_store.remove(conn, uid) catch |err|
            std.log.warn("page-suite cleanup ignored: {s}", .{@errorName(err)});
    }
}

/// The byte offset of `needle` in `haystack`, or an error naming what was missing.
fn indexOfOrFail(haystack: []const u8, needle: []const u8) !usize {
    return std.mem.indexOf(u8, haystack, needle) orelse {
        std.log.warn("expected {s} in body={s}", .{ needle, haystack });
        return error.NeedleNotInBody;
    };
}

/// `a` must appear before `b`. Substring positions ARE the order assertion: the
/// page is one JSON array, so an earlier offset is an earlier element.
fn expectOrder(body: []const u8, a: []const u8, b: []const u8) !void {
    try std.testing.expect(try indexOfOrFail(body, a) < try indexOfOrFail(body, b));
}

/// `{MODELS_PATH}?q={SUITE_NEEDLE}&...` — every request scopes to this suite's
/// rows so a sibling's catalogue entries cannot shift the assertions.
fn suitePath(alloc: std.mem.Allocator, extra: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, MODELS_PATH ++ "?q=" ++ SUITE_NEEDLE ++ "{s}", .{extra});
}

test "integration: test_model_page_and_conditional_headers — the normalized keyset orders, ties, and resumes exactly" {
    const alloc = std.testing.allocator;
    const h = try openOrSkip(alloc);
    defer h.deinit();
    try seed(h);
    defer cleanup(h);

    // ── page one of two ──────────────────────────────────────────────────────
    const first_path = try suitePath(alloc, "&limit=2");
    defer alloc.free(first_path);
    const cursor = blk: {
        const r = try (try h.get(first_path).bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        // Both rows of the display-key TIE, in vendor order. This is the pair
        // that a single-key sort or a reversed vendor comparison gets wrong.
        try expectOrder(r.body, VENDOR_ALPHA, VENDOR_BETA);
        try std.testing.expect(r.bodyContains(MODEL_ALPHA));
        // Page two's rows must NOT be here — a limit that silently over-served
        // would still satisfy every ordering assertion above.
        try std.testing.expect(!r.bodyContains(MODEL_MID));
        try std.testing.expect(!r.bodyContains(MODEL_ZETA));
        try std.testing.expect(!r.bodyContains("\"next_cursor\":null"));

        break :blk try extractCursor(alloc, r.body);
    };
    defer alloc.free(cursor);

    // ── page two, resumed from the cursor ────────────────────────────────────
    const rest = try std.fmt.allocPrint(alloc, "&limit=2&starting_after={s}", .{cursor});
    defer alloc.free(rest);
    const second_path = try suitePath(alloc, rest);
    defer alloc.free(second_path);

    const r = try (try h.get(second_path).bearer(VIEWER)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try expectOrder(r.body, MODEL_MID, MODEL_ZETA);
    // The boundary is EXCLUSIVE. An inclusive seek would repeat the tied pair,
    // and a row count alone would not notice because the page would still be
    // full — which is why the identity is asserted rather than the length.
    try std.testing.expect(!r.bodyContains(MODEL_ALPHA));
    try std.testing.expect(r.bodyContains("\"next_cursor\":null"));
}

/// Pull `next_cursor`'s value out of the envelope. Deliberately string-scraped
/// rather than JSON-parsed: the cursor is opaque to a client, and this test is
/// a client.
fn extractCursor(alloc: std.mem.Allocator, body: []const u8) ![]u8 {
    const key = "\"next_cursor\":\"";
    const start = (try indexOfOrFail(body, key)) + key.len;
    const end = std.mem.indexOfScalarPos(u8, body, start, '"') orelse return error.UnterminatedCursor;
    return alloc.dupe(u8, body[start..end]);
}

test "integration: test_model_page_and_conditional_headers — filters select, and LIKE wildcards match literally" {
    const alloc = std.testing.allocator;
    const h = try openOrSkip(alloc);
    defer h.deinit();
    try seed(h);
    defer cleanup(h);

    {
        // Provider filter is an exact normalized match, not a substring: the
        // vendor names share a prefix, so a LIKE here would return all three.
        const path = try suitePath(alloc, "&provider=" ++ VENDOR_GAMMA);
        defer alloc.free(path);
        const r = try (try h.get(path).bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains(MODEL_MID));
        try std.testing.expect(!r.bodyContains(MODEL_ZETA));
        try std.testing.expect(!r.bodyContains(MODEL_ALPHA));
    }
    {
        // An unknown provider is VALID and simply matches nothing (§2), rather
        // than a 400 — the catalogue's vendor column is arbitrary text.
        const path = try suitePath(alloc, "&provider=m143page-vendor-nonexistent");
        defer alloc.free(path);
        const r = try (try h.get(path).bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("\"models\":[]"));
    }
    {
        // `%` is escaped, so it matches a literal percent — which none of the
        // seeded ids contain. Unescaped it would be "match everything", and the
        // page would come back full.
        const r = try (try h.get(MODELS_PATH ++ "?q=m143page%25").bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(!r.bodyContains(MODEL_ALPHA));
    }
    {
        // Fullwidth ％ (U+FF05, URL-encoded %EF%BC%85) NFKC-folds into `%`
        // INSIDE Postgres — after Zig-side escaping used to run, which made it
        // a live match-everything wildcard. The pattern is now escaped in SQL
        // after the fold, so it matches a literal percent like its ASCII twin.
        const r = try (try h.get(MODELS_PATH ++ "?q=m143page%EF%BC%85").bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(!r.bodyContains(MODEL_ALPHA));
    }
}

test "integration: test_model_page_and_conditional_headers — both answers carry the validators, and If-None-Match yields a bodyless 304" {
    const alloc = std.testing.allocator;
    const h = try openOrSkip(alloc);
    defer h.deinit();
    try seed(h);
    defer cleanup(h);

    const path = try suitePath(alloc, "&limit=2");
    defer alloc.free(path);

    const tag = blk: {
        const r = try (try h.get(path).bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try expectValidators(r);
        break :blk try alloc.dupe(u8, r.header(etag.HEADER_ETAG) orelse return error.NoETag);
    };
    defer alloc.free(tag);

    // Strong, weak, and wildcard all revalidate to 304. The weak form is the
    // one that distinguishes If-None-Match from If-Match: a cache that stored
    // the body under `W/"x"` must still be told 304 when the tag is `"x"`, or
    // it re-downloads a payload it already holds.
    const weak = try std.fmt.allocPrint(alloc, "W/{s}", .{tag});
    defer alloc.free(weak);
    for ([_][]const u8{ tag, weak, "*" }) |candidate| {
        const r = try (try (try h.get(path).bearer(VIEWER))
            .header(etag.HEADER_IF_NONE_MATCH, candidate)).send();
        defer r.deinit();
        try r.expectStatus(.not_modified);
        // RFC 9110: a 304 carries no body. It must still carry the validators —
        // omitting them tells a cache to stop revalidating the very
        // representation it just revalidated.
        try std.testing.expectEqual(@as(usize, 0), r.body.len);
        try expectValidators(r);
    }

    // A tag that does not match is a full 200, not a wrong 304 — the single
    // failure mode of a conditional read is answering 304 with the caller
    // holding something else.
    {
        const r = try (try (try h.get(path).bearer(VIEWER))
            .header(etag.HEADER_IF_NONE_MATCH, "\"m143page-not-the-tag\"")).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.body.len > 0);
    }
}

fn expectValidators(r: harness_mod.Response) !void {
    try std.testing.expect(r.header(etag.HEADER_ETAG) != null);
    // pin test: literals are the §2 contract — a shared cache keying on the
    // wrong header, or missing Vary, serves one tenant's page to another.
    try std.testing.expectEqualStrings("private, no-cache", r.header("Cache-Control") orelse return error.NoCacheControl);
    try std.testing.expectEqualStrings("Authorization", r.header("Vary") orelse return error.NoVary);
}

test "integration: test_model_page_and_conditional_headers — every §Error Contracts 400 fires with no unpaged fallback" {
    const alloc = std.testing.allocator;
    const h = try openOrSkip(alloc);
    defer h.deinit();

    // `limit` outside 1..100 and an over-long `q` are both UZ-LIBRARY-003.
    const long_q = try alloc.alloc(u8, 129);
    defer alloc.free(long_q);
    @memset(long_q, 'q');
    const long_q_path = try std.fmt.allocPrint(alloc, MODELS_PATH ++ "?q={s}", .{long_q});
    defer alloc.free(long_q_path);

    for ([_][]const u8{
        MODELS_PATH ++ "?limit=0",
        MODELS_PATH ++ "?limit=101",
        MODELS_PATH ++ "?limit=notanumber",
        long_q_path,
    }) |path| {
        const r = try (try h.get(path).bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS);
        // Never falls back to an unpaged read: a rejected bound that still
        // returned rows would be worse than the unbounded page §2 removed.
        try std.testing.expect(!r.bodyContains("\"models\""));
    }

    // A cursor that will not decode is UZ-LIBRARY-001 — not something this
    // endpoint issued.
    {
        const r = try (try h.get(MODELS_PATH ++ "?starting_after=not-a-cursor").bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_LIBRARY_CURSOR_MALFORMED);
    }

    // A cursor that DECODES cleanly but names a non-UUID id is the same
    // UZ-LIBRARY-001: the uid rides the page SQL as a `::uuid` cast, and a
    // hand-minted id must be rejected as malformed input rather than surface
    // as a Postgres cast error dressed in a 503.
    {
        const forged = try pagination.encode(alloc, catalogue_key.Cursor, .{
            .display_key = "aaa",
            .vendor_key = "aaa",
            .id = "not-a-uuid",
            .q = null,
            .provider = null,
            .limit = pagination.DEFAULT_LIMIT,
        });
        defer alloc.free(forged);
        const path = try std.fmt.allocPrint(alloc, MODELS_PATH ++ "?starting_after={s}", .{forged});
        defer alloc.free(path);
        const r = try (try h.get(path).bearer(VIEWER)).send();
        defer r.deinit();
        try r.expectStatus(.bad_request);
        try r.expectErrorCode(ec.ERR_LIBRARY_CURSOR_MALFORMED);
    }
}
