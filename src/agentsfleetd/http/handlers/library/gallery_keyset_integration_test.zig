//! Integration tier for §3 Dimension 3.1 — `test_fleet_keyset_and_detail_status`.
//!
//! One claim over one fixture set: the merged gallery's three-part order
//! resumes exactly across a page boundary. (A per-entry detail route existed
//! here once — built for a dashboard click-through that never landed — and was
//! removed; a pin below holds its former URL to "no such route".)
//!
//! ## Every fixture ties, because the order is unreachable otherwise
//!
//! §3 sorts `created_at DESC, tier_rank ASC, id COLLATE "C" DESC` — and each
//! comparison is DEAD CODE until the one before it ties. A fixture set with four
//! distinct timestamps passes against a single-key sort, against a reversed tier
//! rank, and against a reversed id tiebreak, all three. So three rows here share
//! one `created_at`, and two of those additionally share a tier.
//!
//! The directions differ from each other, which is the part that gets written
//! backwards: `created_at` and `id` descend, `tier_rank` ascends. A seek
//! predicate that disagrees with its ORDER BY does not error — it silently skips
//! or repeats rows at page boundaries, which is why the page-two assertions name
//! rows rather than count them.
//!
//! ## The page boundary is placed deliberately
//!
//! With `limit=2`, page one ends on the SECOND of the two rows that share both
//! `created_at` and `tier_rank`. Resuming therefore exercises the third seek
//! clause — the id comparison — which is the one no other arrangement reaches.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const http_auth = @import("../../../db/test_fixtures_http_auth.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const library_store = @import("../../../fleet_library/library_store.zig");

const TOKEN = scope_fixtures.TENANT_ADMIN;

/// Platform ids are TEXT, compared bytewise under `COLLATE "C"` and sorted
/// DESCENDING, so the `-b` row precedes the `-a` row.
///
/// Named for their BYTES rather than for any meaning. The first draft called
/// them `hi`/`lo` for their timestamps and then asserted them in that order —
/// but `"…-lo" > "…-hi"` bytewise ('l' is 0x6c, 'h' is 0x68), so the test failed
/// while the product was right. A fixture name that describes anything other
/// than the sort key is an invitation to assert the wrong order.
const P_NEW = "m143fk-p-new";
const P_TIE_B = "m143fk-p-tie-b";
const P_TIE_A = "m143fk-p-tie-a";

/// Tenant ids are UUIDs — the UNION casts them to text so both arms share one
/// comparable id column.
const T_MINE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ab001";
const T_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ab002";

/// Two timestamps only: the older one is shared by three rows, which is what
/// makes the second and third sort keys reachable at all.
const TS_NEW: i64 = 2000;
const TS_OLD: i64 = 1000;

/// Any non-null value will do — the platform arm filters on `content_hash IS
/// NOT NULL`, never on its content.
const CONTENT_HASH = "0000000000000000000000000000000000000000000000000000000000000143";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn openOrSkip(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

/// Both libraries emptied, then rebuilt. The platform catalogue is global, and
/// this suite asserts an exact ORDER rather than mere membership, so a sibling
/// suite's leftover row would not merely add an entry — it would move one.
fn seed(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    _ = try conn.exec("DELETE FROM core.tenant_fleet_library", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_SECONDARY);

    // Inserted out of sort order, so a passing assertion cannot be insertion
    // order wearing the ORDER BY's clothes.
    try seedPlatform(conn, P_TIE_A, TS_OLD);
    try seedPlatform(conn, P_NEW, TS_NEW);
    try seedPlatform(conn, P_TIE_B, TS_OLD);
    try seedTenant(conn, T_MINE, http_auth.WS_PRIMARY, TS_OLD);
    // Same timestamp, same tier, DIFFERENT workspace. Its only job is to be
    // absent: workspace scoping is a property of the tenant arm's WHERE, and a
    // fixture set without a foreign row cannot tell a scoped query from an
    // unscoped one.
    try seedTenant(conn, T_FOREIGN, http_auth.WS_SECONDARY, TS_OLD);
}

/// Visibility comes from `library_store.VISIBILITY_PUBLIC`, not a literal. The
/// first draft of this fixture spelled it `published`, which is not a value the
/// enum carries — every platform row was invisible and the suite failed as
/// though the ORDER BY were wrong. The store's own doc says these literals are
/// "asserted, not assumed"; a test that re-spells them is the assumption.
fn seedPlatform(conn: *pg.Conn, id: []const u8, created_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_library
        \\  (id, name, description, source_repo, source_path, source_ref,
        \\   required_credentials, required_credentials_reasons, required_tools,
        \\   network_hosts, visibility, content_hash, created_at, updated_at)
        \\VALUES ($1, $1, 'keyset fixture', $1, '', 'main',
        \\        '[]'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, $2, $3, $4, $4)
    , .{ id, library_store.VISIBILITY_PUBLIC, CONTENT_HASH, created_at });
}

/// `requirements_json` carries all FOUR keys `entry_view.Requirements` declares,
/// `trigger_present` included.
///
/// The gallery arm reads the blob field-by-field and COALESCEs that one, so a
/// three-key blob pages fine — but the DETAIL route parses the whole blob into
/// the struct, where a missing non-optional field is a parse error and surfaces
/// as `UZ-INTERNAL-003`. The first draft omitted it and the summary passed while
/// the detail 500'd, which is exactly why this suite drives both.
fn seedTenant(conn: *pg.Conn, id: []const u8, workspace_id: []const u8, created_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.tenant_fleet_library
        \\  (id, workspace_id, name, description, source_kind, source_ref, visibility,
        \\   content_hash, skill_markdown, trigger_markdown, support_files_json,
        \\   requirements_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'keyset fixture', 'upload', 'unit', 'tenant',
        \\        $1, '# skill', NULL, '[]'::jsonb,
        \\        '{"credentials":[],"tools":[],"network_hosts":[],"trigger_present":false}'::jsonb,
        \\        $4, $4)
    , .{ id, workspace_id, id, created_at });
}

fn galleryUrl(alloc: std.mem.Allocator, extra: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleet-libraries{s}", .{ http_auth.WS_PRIMARY, extra });
}

fn indexOfOrFail(haystack: []const u8, needle: []const u8) !usize {
    return std.mem.indexOf(u8, haystack, needle) orelse {
        std.log.warn("expected {s} in body={s}", .{ needle, haystack });
        return error.NeedleNotInBody;
    };
}

/// Substring offsets ARE the order assertion: the page is one JSON array, so an
/// earlier offset is an earlier element.
fn expectOrder(body: []const u8, first: []const u8, second: []const u8) !void {
    try std.testing.expect(try indexOfOrFail(body, first) < try indexOfOrFail(body, second));
}

fn extractCursor(alloc: std.mem.Allocator, body: []const u8) ![]u8 {
    const key = "\"next_cursor\":\"";
    const start = (try indexOfOrFail(body, key)) + key.len;
    const end = std.mem.indexOfScalarPos(u8, body, start, '"') orelse return error.UnterminatedCursor;
    return alloc.dupe(u8, body[start..end]);
}

test "integration: test_fleet_keyset_and_detail_status — the merged order ties on every key and resumes exclusively" {
    const alloc = std.testing.allocator;
    const h = openOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seed(conn);
    }

    // ── the whole page, unpaged: all three keys in one sequence ──────────────
    {
        const url = try galleryUrl(alloc, "");
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        // created_at DESC — the newest row leads.
        try expectOrder(r.body, P_NEW, P_TIE_B);
        // id DESC within the created_at + tier_rank tie. Reversed, this puts
        // P_TIE_A first and nothing else in the suite notices.
        try expectOrder(r.body, P_TIE_B, P_TIE_A);
        // tier_rank ASC — platform (0) before tenant (1) at equal created_at.
        // The one key that ascends while its neighbours descend.
        try expectOrder(r.body, P_TIE_A, T_MINE);

        // Workspace scoping: the foreign tenant row shares this timestamp and
        // tier, so its absence is about the WHERE and not about the ordering.
        try std.testing.expect(!r.bodyContains(T_FOREIGN));
    }

    // ── paged: the boundary lands mid-tie, so the id clause decides ──────────
    const cursor = blk: {
        const url = try galleryUrl(alloc, "?limit=2");
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try expectOrder(r.body, P_NEW, P_TIE_B);
        try std.testing.expect(!r.bodyContains(P_TIE_A));
        try std.testing.expect(!r.bodyContains(T_MINE));
        break :blk try extractCursor(alloc, r.body);
    };
    defer alloc.free(cursor);

    const rest = try std.fmt.allocPrint(alloc, "?limit=2&starting_after={s}", .{cursor});
    defer alloc.free(rest);
    const url = try galleryUrl(alloc, rest);
    defer alloc.free(url);
    const r = try (try h.get(url).bearer(TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    try expectOrder(r.body, P_TIE_A, T_MINE);
    // EXCLUSIVE. An inclusive `<=` on the id clause repeats P_TIE_B here, and the
    // page would still be full — so a row count would not catch it.
    try std.testing.expect(!r.bodyContains(P_TIE_B));
    try std.testing.expect(!r.bodyContains(P_NEW));
    try std.testing.expect(r.bodyContains("\"next_cursor\":null"));
}

fn detailUrl(alloc: std.mem.Allocator, workspace: []const u8, tier: []const u8, id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleet-libraries/{s}/{s}", .{ workspace, tier, id });
}

test "integration: test_fleet_keyset_and_detail_status — the removed detail URL is no route, even for an entry that exists" {
    const alloc = std.testing.allocator;
    const h = openOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seed(conn);
    }

    // The per-entry detail route was removed with its handler — no product
    // caller was ever built, and `support_files` lives on the admin plane
    // only. Its former URL must fall through the router entirely, for a row
    // that IS resident: a stale table arm or a half-resurrected matcher would
    // answer something here, and this pin is what catches it.
    {
        const url = try detailUrl(alloc, http_auth.WS_PRIMARY, "platform", P_NEW);
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
    }
    {
        const url = try detailUrl(alloc, http_auth.WS_PRIMARY, "tenant", T_MINE);
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
    }
}

test "test_library_reads_ignore_retired_search_param: a stray q leaves the gallery byte-identical" {
    // The models route has this proof; the gallery is the other read the
    // retired parameter was stripped from, and a bookmarked ?q= must be
    // ignored — same rows, same cursor, no 400.
    const alloc = std.testing.allocator;
    const h = try openOrSkip(alloc);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seed(conn);

    const base_url = try galleryUrl(alloc, "");
    defer alloc.free(base_url);
    const base = try (try h.get(base_url).bearer(TOKEN)).send();
    defer base.deinit();
    try base.expectStatus(.ok);

    const variants = [_][]const u8{ "?q=alpha", "?q=%25", "?q=" };
    for (variants) |variant| {
        const url = try galleryUrl(alloc, variant);
        defer alloc.free(url);
        const r = try (try h.get(url).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expectEqualStrings(base.body, r.body);
    }
}
