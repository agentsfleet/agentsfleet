//! The platform catalog's upload source, against the real schema.
//!
//! The operator dialog posts an upload with NO `source_ref`, so the row stores an
//! empty `source_repo` — and `INSERT_PLATFORM`'s conflict guard is a comparison
//! ON that column (`WHERE $15::boolean OR core.fleet_library.source_repo =
//! EXCLUDED.source_repo`). Two behaviours fall out of that comparison, and the
//! dialog is built on both:
//!
//!   * re-uploading a corrected bundle under the same name is an UPDATE, not a
//!     collision — otherwise every ordinary re-upload would demand `replace`;
//!   * a name a repository already owns still collides, so the operator's
//!     confirm-and-retry has to carry `replace` on the upload request too, or
//!     the Replace button retries into the same refusal forever.
//!
//! `catalog_integration_test.zig` covers the guard for an upload that DOES carry
//! a `source_ref`; neither of the two cases above is reachable from there.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const scope_fixtures = @import("../../test_scope_tokens.zig");
const http_auth = @import("../../../db/test_fixtures_http_auth.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TOKEN_PLATFORM = scope_fixtures.PLATFORM_ADMIN;
const PLATFORM_URL = "/v1/admin/fleet-libraries";

const PROBE_ID = "upload-probe";
/// The repository an incumbent row was fetched from. An upload stores the empty
/// string here, which is exactly what makes the two differ under the guard.
const INCUMBENT_REPO = "agentsfleet/upload-probe";
const NO_REPO = "";

const SKILL_FIRST =
    \\---
    \\name: upload-probe
    \\description: First body.
    \\version: 0.1.0
    \\---
    \\First body for the upload probe.
;

const SKILL_SECOND =
    \\---
    \\name: upload-probe
    \\description: Corrected body.
    \\version: 0.2.0
    \\---
    \\Corrected body for the upload probe.
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

/// Post one upload. `source_ref` empty is what the operator dialog sends; a
/// non-empty one stands in for a row that was fetched from a repository, since
/// the guard compares the stored string and not how it got there.
fn upload(
    h: *TestHarness,
    alloc: std.mem.Allocator,
    source_ref: []const u8,
    skill: []const u8,
    replace: bool,
) !harness_mod.Response {
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = source_ref,
        .skill_markdown = skill,
        .replace = replace,
    }, .{});
    defer alloc.free(body);
    return (try (try h.post(PLATFORM_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
}

/// The stored source and content hash, copied out before the result is drained.
fn readRow(conn: *pg.Conn, alloc: std.mem.Allocator) !struct { repo: []const u8, hash: []const u8 } {
    var q = PgQuery.from(try conn.query(
        "SELECT source_repo, content_hash FROM core.fleet_library WHERE id = $1",
        .{PROBE_ID},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    // Both slices are row-backed and die with the result, so they are copied
    // before it drains. The errdefer covers the window where the first copy
    // succeeded and the second did not — the testing allocator counts that leak.
    const repo = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(repo);
    const hash = try alloc.dupe(u8, try row.get([]const u8, 1));
    return .{ .repo = repo, .hash = hash };
}

fn countRows(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query("SELECT COUNT(*) FROM core.fleet_library", .{}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    return row.get(i64, 0);
}

test "integration: re-uploading a corrected bundle under one name updates it in place" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);

    const first = try upload(h, alloc, NO_REPO, SKILL_FIRST, false);
    defer first.deinit();
    try first.expectStatus(.created);

    const before = try readRow(conn, alloc);
    defer alloc.free(before.repo);
    defer alloc.free(before.hash);
    try std.testing.expectEqualStrings(NO_REPO, before.repo);

    // The correction an operator makes on disk and uploads again. Both rows carry
    // the empty source, so the guard's comparison holds and this is the update
    // path — never the 409 that would make `replace` a routine gesture.
    const second = try upload(h, alloc, NO_REPO, SKILL_SECOND, false);
    defer second.deinit();
    try second.expectStatus(.created);

    const after = try readRow(conn, alloc);
    defer alloc.free(after.repo);
    defer alloc.free(after.hash);
    try std.testing.expect(!std.mem.eql(u8, before.hash, after.hash));
    try std.testing.expectEqual(@as(i64, 1), try countRows(conn));
}

test "integration: an upload cannot take a name a repository owns until replace is said" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);

    const incumbent = try upload(h, alloc, INCUMBENT_REPO, SKILL_FIRST, false);
    defer incumbent.deinit();
    try incumbent.expectStatus(.created);

    const collision = try upload(h, alloc, NO_REPO, SKILL_SECOND, false);
    defer collision.deinit();
    try collision.expectStatus(.conflict);
    try collision.expectErrorCode("UZ-CATALOG-004");

    const held = try readRow(conn, alloc);
    defer alloc.free(held.repo);
    defer alloc.free(held.hash);
    try std.testing.expectEqualStrings(INCUMBENT_REPO, held.repo);

    // Saying it out loud is what the dialog's Replace button sends — and it has to
    // reach the wire on the upload branch, or that button retries the refusal.
    const replaced = try upload(h, alloc, NO_REPO, SKILL_SECOND, true);
    defer replaced.deinit();
    try replaced.expectStatus(.created);

    const taken = try readRow(conn, alloc);
    defer alloc.free(taken.repo);
    defer alloc.free(taken.hash);
    try std.testing.expectEqualStrings(NO_REPO, taken.repo);
    try std.testing.expectEqual(@as(i64, 1), try countRows(conn));
}
