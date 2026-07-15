//! Integration coverage for optimistic concurrency on the catalog row (M131 §9)
//! — the shared `http/etag.zig` capability's SECOND adopter.
//!
//! The claims, each true only if the If-Match verdict runs against the real row:
//!   - A stale `If-Match` is a 412 UZ-CATALOG-005 carrying the current etag, and
//!       nothing is written.
//!   - The destructive case: an operator's stale form re-sending `source_repo`
//!       after another operator moved the row is REFUSED — so the bundle
//!       (`content_hash`) is not discarded out from under it.
//!   - `If-Match` is opt-in: a PATCH without it still succeeds (last-write-wins),
//!       so existing callers are unbroken.
//!
//! Upload-kind onboards throughout — no network, no object storage.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const scope_fixtures = @import("../../test_scope_tokens.zig");
const http_auth = @import("../../../db/test_fixtures_http_auth.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TOKEN_PLATFORM = scope_fixtures.PLATFORM_ADMIN;
const CATALOG_URL = "/v1/admin/fleet-libraries";
const IF_MATCH = "if-match";

const PROBE_ID = "etag-probe";
const PROBE_REPO = "agentsfleet/etag-probe";
const MOVED_REPO = "agentsfleet/etag-probe-moved";
const PROBE_SKILL =
    \\---
    \\name: etag-probe
    \\description: Bundle-derived description.
    \\version: 0.1.0
    \\---
    \\Body for the etag probe.
;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn reset(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

fn addProbe(h: *TestHarness, alloc: std.mem.Allocator) !void {
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = PROBE_REPO,
        .skill_markdown = PROBE_SKILL,
        .replace = false,
    }, .{});
    defer alloc.free(body);
    const res = try (try (try h.post(CATALOG_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
    defer res.deinit();
    try res.expectStatus(.created);
}

/// The catalog list's `etag` for the probe — the tag a row editor would send as
/// `If-Match`. Caller frees.
fn probeEtag(h: *TestHarness, alloc: std.mem.Allocator) ![]const u8 {
    const res = try (try h.get(CATALOG_URL).bearer(TOKEN_PLATFORM)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, res.body, .{});
    defer parsed.deinit();
    for (parsed.value.object.get("entries").?.array.items) |entry| {
        if (std.mem.eql(u8, entry.object.get("id").?.string, PROBE_ID)) {
            const tag = entry.object.get("etag") orelse return error.NoEtagOnEntry;
            return alloc.dupe(u8, tag.string);
        }
    }
    return error.ProbeMissing;
}

fn entryUrl(alloc: std.mem.Allocator, id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}", .{ CATALOG_URL, id });
}

fn hasBundle(conn: *pg.Conn, id: []const u8) !bool {
    var q = PgQuery.from(try conn.query("SELECT content_hash FROM core.fleet_library WHERE id = $1", .{id}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    return (try row.get(?[]const u8, 0)) != null;
}

fn readSourceRepo(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query("SELECT source_repo FROM core.fleet_library WHERE id = $1", .{id}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

test "integration: catalog stale If-Match → 412 with current etag, nothing written" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);
    try addProbe(h, alloc);

    const current = try probeEtag(h, alloc);
    defer alloc.free(current);

    const url = try entryUrl(alloc, PROBE_ID);
    defer alloc.free(url);
    const body = "{\"description\":\"edited by a racer\"}";

    // Stale tag → 412 UZ-CATALOG-005, body carries the current etag, row unchanged.
    const r_stale = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(body)).header(IF_MATCH, "\"deadbeef\"")).send();
    defer r_stale.deinit();
    try r_stale.expectStatus(.precondition_failed);
    try r_stale.expectErrorCode("UZ-CATALOG-005");
    try std.testing.expect(r_stale.bodyContains(current)); // the 412 hands back the real tag

    // The matching tag → 200 (the reloaded save).
    const r_ok = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(body)).header(IF_MATCH, current)).send();
    defer r_ok.deinit();
    try r_ok.expectStatus(.ok);
}

test "integration: a stale re-send of source_repo cannot discard the bundle" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);
    try addProbe(h, alloc);
    try std.testing.expect(try hasBundle(conn, PROBE_ID)); // onboard stored a bundle

    // Operator A loads the row.
    const etag_a = try probeEtag(h, alloc);
    defer alloc.free(etag_a);

    // Operator B renames the row (matching tag) — the row moves past etag_a.
    const url = try entryUrl(alloc, PROBE_ID);
    defer alloc.free(url);
    const etag_b = try probeEtag(h, alloc); // same version as A here
    defer alloc.free(etag_b);
    const r_b = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json("{\"description\":\"moved on by B\"}")).header(IF_MATCH, etag_b)).send();
    defer r_b.deinit();
    try r_b.expectStatus(.ok);

    // Operator A, still holding the stale tag, tries to repoint the source — the
    // destructive op: without If-Match this repoint would null content_hash and
    // draft the row. With the stale tag it is REFUSED (412), so the bundle survives.
    const moved_body = try std.json.Stringify.valueAlloc(alloc, .{ .source_repo = MOVED_REPO }, .{});
    defer alloc.free(moved_body);
    const r_a = try (try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(moved_body)).header(IF_MATCH, etag_a)).send();
    defer r_a.deinit();
    try r_a.expectStatus(.precondition_failed);

    // The bundle was NOT discarded and the source was NOT repointed.
    try std.testing.expect(try hasBundle(conn, PROBE_ID));
    const repo = try readSourceRepo(alloc, conn, PROBE_ID);
    defer alloc.free(repo);
    try std.testing.expectEqualStrings(PROBE_REPO, repo);
}

test "integration: catalog PATCH without If-Match still succeeds (opt-in)" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);
    try addProbe(h, alloc);

    const url = try entryUrl(alloc, PROBE_ID);
    defer alloc.free(url);
    // No If-Match header at all → last-write-wins, unchanged from pre-M131.
    const r = try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json("{\"description\":\"blind save\"}")).send();
    defer r.deinit();
    try r.expectStatus(.ok);
}
