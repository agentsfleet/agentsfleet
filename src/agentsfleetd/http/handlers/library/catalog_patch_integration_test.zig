//! Integration coverage for the widened PATCH (M130 §2/§3/§4).
//!
//! Three claims, each of which is only true if the SQL is right — a unit test on
//! the handler cannot prove any of them, because the behaviour lives inside the
//! statements:
//!
//!   §2  Repointing the source DISCARDS the bundle. `content_hash` goes NULL and
//!       `visibility` falls to draft, together, in one statement. Re-sending the
//!       SAME source does neither — the dialog echoes every field back, so
//!       treating "present" as "changed" would withdraw a live fleet on a copy
//!       edit.
//!
//!   §3  An operator's rename SURVIVES the next refetch. `name` left
//!       INSERT_PLATFORM's ON CONFLICT SET to make that true.
//!
//!   §4  A refetch PRUNES the reason map to the credentials the incoming bundle
//!       actually declares — a departed credential must not leave a dead key that
//!       the dialog never renders and every save faithfully round-trips.
//!
//! Sibling file to catalog_integration_test.zig, which owns the M128 lifecycle
//! guards; this one owns the M130 write. Upload-kind onboards throughout — no
//! network, no object storage.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const scope_fixtures = @import("../../test_scope_tokens.zig");
const http_auth = @import("../../../db/test_fixtures_http_auth.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_PLATFORM = scope_fixtures.PLATFORM_ADMIN;

const CATALOG_URL = "/v1/admin/fleet-libraries";

const PROBE_ID = "patch-probe";
const PROBE_REPO = "agentsfleet/patch-probe";
const MOVED_REPO = "agentsfleet/patch-probe-moved";

const OPERATOR_NAME = "Operator's own name";

/// The probe's bundle declares ONE credential. §4's pruning is only observable
/// against a bundle that declares a DIFFERENT set, which
/// `PROBE_SKILL_SWAPPED_CREDS` supplies.
const PROBE_SKILL =
    \\---
    \\name: patch-probe
    \\description: Bundle-derived description.
    \\version: 0.1.0
    \\credentials:
    \\  - GITHUB_TOKEN
    \\---
    \\Body for the patch probe.
;

/// Same slug, a DIFFERENT declared credential set: GITHUB_TOKEN departs, SLACK
/// arrives. A refetch of this must drop the GITHUB_TOKEN reason and keep nothing
/// stale behind.
const PROBE_SKILL_SWAPPED_CREDS =
    \\---
    \\name: patch-probe
    \\description: Bundle-derived description.
    \\version: 0.1.0
    \\credentials:
    \\  - SLACK_TOKEN
    \\---
    \\Body for the patch probe.
;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn reset(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

fn entryUrl(alloc: std.mem.Allocator, id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}", .{ CATALOG_URL, id });
}

fn addFleet(h: *TestHarness, alloc: std.mem.Allocator, repo: []const u8, skill: []const u8) !harness_mod.Response {
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = repo,
        .skill_markdown = skill,
        .replace = false,
    }, .{});
    defer alloc.free(body);
    return (try (try h.post(CATALOG_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
}

fn addProbe(h: *TestHarness, alloc: std.mem.Allocator) !void {
    const res = try addFleet(h, alloc, PROBE_REPO, PROBE_SKILL);
    defer res.deinit();
    try res.expectStatus(.created);
}

fn patchEntry(h: *TestHarness, alloc: std.mem.Allocator, id: []const u8, body: []const u8) !harness_mod.Response {
    const url = try entryUrl(alloc, id);
    defer alloc.free(url);
    return (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(body)).send();
}

fn publish(h: *TestHarness, alloc: std.mem.Allocator, id: []const u8) !void {
    const res = try patchEntry(h, alloc, id, "{\"published\":true}");
    defer res.deinit();
    try res.expectStatus(.ok);
}

const Row = struct {
    visibility: []const u8,
    has_bundle: bool,
    name: []const u8,
    source_repo: []const u8,
    source_ref: []const u8,
    reasons: []const u8,
};

/// Read the row back FROM POSTGRES, not from the response body. The response is
/// the handler's account of what it did; the table is what actually happened, and
/// this spec is entirely about the gap between those two.
fn readRow(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) !Row {
    var q = PgQuery.from(try conn.query(
        \\SELECT visibility, content_hash, name, source_repo, source_ref,
        \\       required_credentials_reasons::text
        \\  FROM core.fleet_library WHERE id = $1
    , .{id}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    const hash = try row.get(?[]const u8, 1);
    return .{
        .visibility = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .has_bundle = hash != null,
        .name = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .source_repo = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .source_ref = try alloc.dupe(u8, try row.get([]const u8, 4)),
        .reasons = try alloc.dupe(u8, try row.get([]const u8, 5)),
    };
}

fn freeRow(alloc: std.mem.Allocator, r: Row) void {
    alloc.free(r.visibility);
    alloc.free(r.name);
    alloc.free(r.source_repo);
    alloc.free(r.source_ref);
    alloc.free(r.reasons);
}

// ── The ref pin survives the round trip ─────────────────────────────────────

test "integration: a pinned ref is stored by the fetch that honors it" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);

    // Upload-kind body carrying an explicit ref — the same field the dashboard's
    // Fetch-update sends with the row's stored pin. The store must record the
    // ref the content came from, not silently reset to the default branch.
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = PROBE_REPO,
        .ref = "v2.1.0",
        .skill_markdown = PROBE_SKILL,
        .replace = false,
    }, .{});
    defer alloc.free(body);
    const res = try (try (try h.post(CATALOG_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
    defer res.deinit();
    try res.expectStatus(.created);

    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expectEqualStrings("v2.1.0", after.source_ref);
}

// ── Dimension 2.3 — a changed source discards the bundle ─────────────────────

test "integration: repointing the repository nulls the bundle and withdraws the fleet" {
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
    try publish(h, alloc, PROBE_ID);

    {
        const before = try readRow(alloc, conn, PROBE_ID);
        defer freeRow(alloc, before);
        try std.testing.expect(before.has_bundle);
        try std.testing.expectEqualStrings("public", before.visibility);
    }

    const body = try std.fmt.allocPrint(alloc, "{{\"source_repo\":\"{s}\"}}", .{MOVED_REPO});
    defer alloc.free(body);
    const res = try patchEntry(h, alloc, PROBE_ID, body);
    defer res.deinit();
    try res.expectStatus(.ok);

    // The bundle in object storage was built from the OLD repository. Keeping it
    // would make the row advertise a source it is not serving — so it is gone, and
    // the row is no longer public. Both, or neither: they move in one statement.
    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expect(!after.has_bundle);
    try std.testing.expectEqualStrings("draft", after.visibility);
    try std.testing.expectEqualStrings(MOVED_REPO, after.source_repo);
}

// ── Dimension 2.4 — an unchanged source is a no-op ───────────────────────────

test "integration: re-sending the SAME repository does not withdraw a live fleet" {
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
    try publish(h, alloc, PROBE_ID);

    // The edit dialog echoes every field back, so a description-only save re-sends
    // the repository the row already has. If "present" meant "changed", saving a
    // typo fix in the copy would take the fleet out of every workspace gallery.
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"source_repo\":\"{s}\",\"description\":\"curated copy\"}}",
        .{PROBE_REPO},
    );
    defer alloc.free(body);
    const res = try patchEntry(h, alloc, PROBE_ID, body);
    defer res.deinit();
    try res.expectStatus(.ok);

    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expect(after.has_bundle);
    try std.testing.expectEqualStrings("public", after.visibility);
}

// ── Dimension 2.2 — the edit path refuses what the add path refuses ──────────

test "integration: a malformed repository is refused and the row is untouched" {
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
    try publish(h, alloc, PROBE_ID);

    // No slash; a traversal segment; an empty half. Each is refused by the SAME
    // validator the import path asks (github_source.parseOwnerRepo), so a
    // repository this rejects is exactly one `Fetch bundle` would reject.
    const bad = [_][]const u8{ "no-slash", "../etc/passwd", "owner/", "/repo", "a/b/c" };
    for (bad) |repo| {
        const body = try std.fmt.allocPrint(alloc, "{{\"source_repo\":\"{s}\"}}", .{repo});
        defer alloc.free(body);
        const res = try patchEntry(h, alloc, PROBE_ID, body);
        defer res.deinit();
        try res.expectStatus(.bad_request);
    }

    // Every refusal left the fleet exactly as it was — still live, still bundled.
    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expect(after.has_bundle);
    try std.testing.expectEqualStrings("public", after.visibility);
    try std.testing.expectEqualStrings(PROBE_REPO, after.source_repo);
}

// ── Dimension 2.1 — the name refusals ────────────────────────────────────────

test "integration: an empty or over-cap name, and a malformed ref, are refused untouched" {
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

    // Empty name: display copy, but never blank — the wall and the gallery both
    // render it.
    {
        const res = try patchEntry(h, alloc, PROBE_ID, "{\"name\":\"\"}");
        defer res.deinit();
        try res.expectStatus(.bad_request);
    }
    // Over the cap: a paste accident must not fill the column.
    {
        const long_name = "n" ** 201;
        const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"{s}\"}}", .{long_name});
        defer alloc.free(body);
        const res = try patchEntry(h, alloc, PROBE_ID, body);
        defer res.deinit();
        try res.expectStatus(.bad_request);
    }
    // A ref that fails validSegment — same charset rules as the import path.
    {
        const res = try patchEntry(h, alloc, PROBE_ID, "{\"source_ref\":\"..\"}");
        defer res.deinit();
        try res.expectStatus(.bad_request);
    }

    // Every refusal left the row exactly as seeded.
    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expectEqualStrings(PROBE_ID, after.name);
    try std.testing.expect(after.has_bundle);
}

// ── Dimension 2.5 — the slug is immutable ────────────────────────────────────

test "integration: a body carrying an id cannot move the slug" {
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

    // The slug is the primary key AND the id a workspace install references as
    // `platform_library_id`. Moving it would orphan every install, so PatchBody
    // has no `id` field and `ignore_unknown_fields` discards one a caller sends.
    const res = try patchEntry(h, alloc, PROBE_ID, "{\"id\":\"hijacked\",\"name\":\"Renamed\"}");
    defer res.deinit();
    try res.expectStatus(.ok);

    // The rename applied; the slug did not move, and no second row appeared.
    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expectEqualStrings("Renamed", after.name);

    var q = PgQuery.from(try conn.query("SELECT count(*)::bigint FROM core.fleet_library", .{}));
    defer q.deinit();
    const row = try q.next() orelse return error.CountMissing;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

// ── Dimension 3.1 — a rename survives the next fetch ─────────────────────────

test "integration: an operator rename survives a refetch; a first import takes the bundle's" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);

    // A FIRST import still takes its name from the bundle's frontmatter.
    try addProbe(h, alloc);
    {
        const seeded = try readRow(alloc, conn, PROBE_ID);
        defer freeRow(alloc, seeded);
        try std.testing.expectEqualStrings(PROBE_ID, seeded.name);
    }

    const rename = try std.fmt.allocPrint(alloc, "{{\"name\":\"{s}\"}}", .{OPERATOR_NAME});
    defer alloc.free(rename);
    {
        const res = try patchEntry(h, alloc, PROBE_ID, rename);
        defer res.deinit();
        try res.expectStatus(.ok);
    }

    // Refetch the SAME repository. `name` left INSERT_PLATFORM's ON CONFLICT SET,
    // so the bundle no longer overwrites it — offering a rename that the next
    // Fetch update silently reverts would be worse than not offering one.
    {
        const res = try addFleet(h, alloc, PROBE_REPO, PROBE_SKILL);
        defer res.deinit();
        try res.expectStatus(.created);
    }

    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expectEqualStrings(OPERATOR_NAME, after.name);
}

// ── Dimension 4.1 — the reason map tracks the declared credential set ────────

test "integration: a refetch prunes reason keys the new bundle no longer declares" {
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

    // Curate copy for the credential the bundle declares today.
    {
        const res = try patchEntry(
            h,
            alloc,
            PROBE_ID,
            "{\"required_credentials_reasons\":{\"GITHUB_TOKEN\":\"to read your pull requests\"}}",
        );
        defer res.deinit();
        try res.expectStatus(.ok);
    }
    {
        const seeded = try readRow(alloc, conn, PROBE_ID);
        defer freeRow(alloc, seeded);
        try std.testing.expect(std.mem.indexOf(u8, seeded.reasons, "GITHUB_TOKEN") != null);
    }

    // Refetch a bundle that declares SLACK_TOKEN instead. GITHUB_TOKEN is gone from
    // the declared set, so its reason is a dead key: the dialog renders only
    // declared credentials, so an operator would never see it — but it seeds its
    // state from the whole map and PATCHes the whole map back, so the corpse would
    // round-trip forever.
    {
        const res = try addFleet(h, alloc, PROBE_REPO, PROBE_SKILL_SWAPPED_CREDS);
        defer res.deinit();
        try res.expectStatus(.created);
    }

    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expect(std.mem.indexOf(u8, after.reasons, "GITHUB_TOKEN") == null);
}

// ── Regression — the M128 guards did not weaken ──────────────────────────────

test "integration: publishing a row whose bundle this same request discards is refused" {
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

    // Repoint AND publish in one call. By the time the publish would apply there is
    // no bundle to serve — which is exactly UZ-CATALOG-002. Reporting it as a race
    // (the SQL guard's zero-row result) would tell the operator the wrong thing.
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"source_repo\":\"{s}\",\"published\":true}}",
        .{MOVED_REPO},
    );
    defer alloc.free(body);
    const res = try patchEntry(h, alloc, PROBE_ID, body);
    defer res.deinit();
    try res.expectStatus(.conflict);

    // Refused whole: neither the repoint nor the publish landed.
    const after = try readRow(alloc, conn, PROBE_ID);
    defer freeRow(alloc, after);
    try std.testing.expectEqualStrings("draft", after.visibility);
    try std.testing.expectEqualStrings(PROBE_REPO, after.source_repo);
}
