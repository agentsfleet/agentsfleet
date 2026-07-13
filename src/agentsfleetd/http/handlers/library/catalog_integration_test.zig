//! Integration coverage for the platform catalog lifecycle (M128 §1–§3):
//! the operator's list, curate, publish/unpublish, and delete, plus the three
//! guards that make the lifecycle mean something.
//!
//! The load-bearing claims here are the ones a unit test cannot make, because
//! they are properties of the real schema and the real router:
//!   * a fleet is never born in SQL — a fresh catalog is EMPTY (Dimension 1.1);
//!   * publish is the only door to a tenant — a draft is absent from the gallery
//!     AND uninstallable by id, so Unpublish is not decoration (§3);
//!   * a refetch never destroys the operator's curated copy (Dimension 1.4);
//!   * the catalog id belongs to whoever holds it — a second repository declaring
//!     the same frontmatter name is a 409, not a silent content swap (1.5).

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
/// Holds `platform-library:write` — the only scope any catalog route accepts.
const TOKEN_PLATFORM = scope_fixtures.PLATFORM_ADMIN;
/// Holds `library:write` (tenant tier) but NOT `platform-library:write`.
const TOKEN_TENANT = scope_fixtures.TENANT_ADMIN;

const CATALOG_URL = "/v1/admin/fleet-libraries";
const BUNDLES_URL = "/v1/fleets/bundles";

const PROBE_ID = "catalog-probe";
const PROBE_REPO = "agentsfleet/catalog-probe";
const OTHER_REPO = "someone-else/catalog-probe";

const PROBE_SKILL =
    \\---
    \\name: catalog-probe
    \\description: Bundle-derived description.
    \\version: 0.1.0
    \\---
    \\Body for the catalog probe.
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

/// The catalog is runtime-owned, so a test starts from an EMPTY table and builds
/// exactly the rows it needs. There is nothing to "restore to un-onboarded" any
/// more — that concept died with the seed.
fn reset(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

fn entryUrl(alloc: std.mem.Allocator, id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}", .{ CATALOG_URL, id });
}

/// Add a fleet from `repo`. Upload kind — no network, no object storage.
fn addFleet(h: *TestHarness, alloc: std.mem.Allocator, repo: []const u8, replace: bool) !harness_mod.Response {
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = repo,
        .skill_markdown = PROBE_SKILL,
        .replace = replace,
    }, .{});
    defer alloc.free(body);
    return (try (try h.post(CATALOG_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
}

fn addProbe(h: *TestHarness, alloc: std.mem.Allocator) !void {
    const res = try addFleet(h, alloc, PROBE_REPO, false);
    defer res.deinit();
    try res.expectStatus(.created);
}

fn patchEntry(h: *TestHarness, alloc: std.mem.Allocator, id: []const u8, body: []const u8) !harness_mod.Response {
    const url = try entryUrl(alloc, id);
    defer alloc.free(url);
    return (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json(body)).send();
}

fn publish(h: *TestHarness, alloc: std.mem.Allocator, id: []const u8) !harness_mod.Response {
    return patchEntry(h, alloc, id, "{\"published\":true}");
}

fn visibilityOf(conn: *pg.Conn, id: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query("SELECT visibility FROM core.fleet_library WHERE id = $1", .{id}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    return row.get([]const u8, 0);
}

fn rowCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query("SELECT count(*)::bigint FROM core.fleet_library", .{}));
    defer q.deinit();
    const row = try q.next() orelse return error.CountMissing;
    return try row.get(i64, 0);
}

// ── Dimension 1.1 — a fleet is never born in SQL ────────────────────────────

test "integration: the migrated catalog is empty — no migration seeds a fleet" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Every migration has run against this database. If any of them still seeded
    // the catalog, this count would be non-zero and Invariant 5 would be a lie.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    try std.testing.expectEqual(@as(i64, 0), try rowCount(conn));

    const res = try (try h.get(CATALOG_URL).bearer(TOKEN_PLATFORM)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(res.bodyContains("\"entries\":[]"));
}

// ── Dimension 2.5 — scope and method ────────────────────────────────────────

test "integration: every catalog route demands platform-library:write" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);

    const listed = try (try h.get(CATALOG_URL).bearer(TOKEN_TENANT)).send();
    defer listed.deinit();
    try listed.expectStatus(.forbidden);

    const url = try entryUrl(alloc, PROBE_ID);
    defer alloc.free(url);

    const patched = try (try (try h.patch(url).bearer(TOKEN_TENANT)).json("{\"published\":true}")).send();
    defer patched.deinit();
    try patched.expectStatus(.forbidden);

    const deleted = try (try h.delete(url).bearer(TOKEN_TENANT)).send();
    defer deleted.deinit();
    try deleted.expectStatus(.forbidden);

    // An unsupported method on a real route is 405, not a 404 or a silent 200.
    const put = try (try (try h.put(url).bearer(TOKEN_PLATFORM)).json("{}")).send();
    defer put.deinit();
    try put.expectStatus(.method_not_allowed);
}

// ── Dimensions 1.3 + 2.1 — a fleet is born from its bundle, as a draft ───────

test "integration: adding a fleet derives the row from the bundle and stages it draft" {
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

    // The id is the bundle's frontmatter name, not the repo the operator typed.
    try std.testing.expectEqualStrings("draft", try visibilityOf(conn, PROBE_ID));

    const res = try (try h.get(CATALOG_URL).bearer(TOKEN_PLATFORM)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(res.bodyContains(PROBE_ID));
    try std.testing.expect(res.bodyContains("Bundle-derived description."));
    try std.testing.expect(res.bodyContains("\"visibility\":\"draft\""));

    // Invariant 3: a catalog read can never carry bundle bodies or storage keys.
    try std.testing.expect(!res.bodyContains("skill_markdown"));
    try std.testing.expect(!res.bodyContains("trigger_markdown"));
    try std.testing.expect(!res.bodyContains("Body for the catalog probe"));
}

// ── Dimension 2.3 — a published row always has a bundle ──────────────────────

test "integration: publishing an entry with no bundle is refused" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);

    // A pre-M128 row: it exists, but no bundle was ever fetched for it. This is
    // exactly the shape a deployed database still carries.
    _ = try conn.exec(
        \\INSERT INTO core.fleet_library
        \\  (id, name, description, source_repo, source_path, source_ref,
        \\   required_credentials, required_credentials_reasons, required_tools,
        \\   network_hosts, visibility, created_at, updated_at)
        \\VALUES ($1, $1, 'no bundle', $2, '', 'main',
        \\        '[]'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, 'draft', 1, 1)
    , .{ PROBE_ID, PROBE_REPO });

    const res = try publish(h, alloc, PROBE_ID);
    defer res.deinit();
    try res.expectStatus(.conflict);
    try res.expectErrorCode("UZ-CATALOG-002");

    // Refused means unchanged, not half-applied.
    try std.testing.expectEqualStrings("draft", try visibilityOf(conn, PROBE_ID));

    // With a bundle, the same publish succeeds.
    try addProbe(h, alloc);
    const ok = try publish(h, alloc, PROBE_ID);
    defer ok.deinit();
    try ok.expectStatus(.ok);
    try std.testing.expectEqualStrings("public", try visibilityOf(conn, PROBE_ID));
}

// ── Dimension 2.4 — a live fleet is never deleted ────────────────────────────

test "integration: deleting a published fleet is refused until it is withdrawn" {
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
    const pub_res = try publish(h, alloc, PROBE_ID);
    defer pub_res.deinit();
    try pub_res.expectStatus(.ok);

    const url = try entryUrl(alloc, PROBE_ID);
    defer alloc.free(url);

    const refused = try (try h.delete(url).bearer(TOKEN_PLATFORM)).send();
    defer refused.deinit();
    try refused.expectStatus(.conflict);
    try refused.expectErrorCode("UZ-CATALOG-003");
    try std.testing.expectEqual(@as(i64, 1), try rowCount(conn));

    // Withdraw, then delete.
    const withdrawn = try patchEntry(h, alloc, PROBE_ID, "{\"published\":false}");
    defer withdrawn.deinit();
    try withdrawn.expectStatus(.ok);

    const gone = try (try h.delete(url).bearer(TOKEN_PLATFORM)).send();
    defer gone.deinit();
    try gone.expectStatus(.no_content);
    try std.testing.expectEqual(@as(i64, 0), try rowCount(conn));
}

// ── Dimensions 2.2 + 1.4 — the operator's copy is theirs, and survives a refetch ──

test "integration: a refetch drafts the row and preserves the operator's curated copy" {
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

    // The operator writes the two fields no bundle can supply, then publishes.
    const curated = try patchEntry(h, alloc, PROBE_ID,
        \\{"description":"Operator copy.","required_credentials_reasons":{"github":"review your pull requests"}}
    );
    defer curated.deinit();
    try curated.expectStatus(.ok);
    try std.testing.expect(curated.bodyContains("Operator copy."));
    // Curating touches nothing the bundle owns.
    try std.testing.expect(curated.bodyContains(PROBE_ID));

    const pub_res = try publish(h, alloc, PROBE_ID);
    defer pub_res.deinit();
    try pub_res.expectStatus(.ok);

    // Refetch the SAME repository — the update path, not a collision.
    try addProbe(h, alloc);

    // The bundle is re-derived, but the operator's copy is untouched, and the
    // fleet is withdrawn to draft rather than shipped to every tenant unreviewed.
    try std.testing.expectEqualStrings("draft", try visibilityOf(conn, PROBE_ID));

    const after = try (try h.get(CATALOG_URL).bearer(TOKEN_PLATFORM)).send();
    defer after.deinit();
    try after.expectStatus(.ok);
    try std.testing.expect(after.bodyContains("Operator copy."));
    try std.testing.expect(after.bodyContains("review your pull requests"));
    // The bundle's own description must NOT have clobbered the operator's.
    try std.testing.expect(!after.bodyContains("Bundle-derived description."));
}

// ── Dimension 1.5 — the catalog id belongs to whoever holds it ───────────────

test "integration: a second repository claiming the same name is a conflict, not a swap" {
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

    // A DIFFERENT repository whose SKILL.md declares the same name. Without the
    // guard this would silently overwrite what every tenant installs.
    const collision = try addFleet(h, alloc, OTHER_REPO, false);
    defer collision.deinit();
    try collision.expectStatus(.conflict);
    try collision.expectErrorCode("UZ-CATALOG-004");

    // The incumbent still owns the row.
    var q = PgQuery.from(try conn.query("SELECT source_repo FROM core.fleet_library WHERE id = $1", .{PROBE_ID}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    try std.testing.expectEqualStrings(PROBE_REPO, try row.get([]const u8, 0));

    // Saying `replace` out loud overwrites it deliberately.
    const replaced = try addFleet(h, alloc, OTHER_REPO, true);
    defer replaced.deinit();
    try replaced.expectStatus(.created);
}

// ── §3 — publish is the only door to a tenant ───────────────────────────────

test "integration: a draft is invisible to tenants and uninstallable by id" {
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

    const gallery_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleet-libraries", .{http_auth.WS_PRIMARY});
    defer alloc.free(gallery_url);

    // Draft: absent from the workspace gallery AND from the public bundles list.
    const hidden = try (try h.get(gallery_url).bearer(TOKEN_TENANT)).send();
    defer hidden.deinit();
    try hidden.expectStatus(.ok);
    try std.testing.expect(!hidden.bodyContains(PROBE_ID));

    const bundles_hidden = try (try h.get(BUNDLES_URL).bearer(TOKEN_TENANT)).send();
    defer bundles_hidden.deinit();
    try bundles_hidden.expectStatus(.ok);
    try std.testing.expect(!bundles_hidden.bodyContains(PROBE_ID));

    // Publishing is what opens the door — in both surfaces at once.
    const pub_res = try publish(h, alloc, PROBE_ID);
    defer pub_res.deinit();
    try pub_res.expectStatus(.ok);

    const shown = try (try h.get(gallery_url).bearer(TOKEN_TENANT)).send();
    defer shown.deinit();
    try std.testing.expect(shown.bodyContains(PROBE_ID));

    const bundles_shown = try (try h.get(BUNDLES_URL).bearer(TOKEN_TENANT)).send();
    defer bundles_shown.deinit();
    try std.testing.expect(bundles_shown.bodyContains(PROBE_ID));

    // Withdrawing closes it again. This is what makes Unpublish mean something
    // rather than merely hiding a row that could still be installed by id.
    const withdrawn = try patchEntry(h, alloc, PROBE_ID, "{\"published\":false}");
    defer withdrawn.deinit();
    try withdrawn.expectStatus(.ok);

    const hidden_again = try (try h.get(gallery_url).bearer(TOKEN_TENANT)).send();
    defer hidden_again.deinit();
    try std.testing.expect(!hidden_again.bodyContains(PROBE_ID));
}

// ── /review finding: a guarded write that touched nothing is NOT a success ───

test "integration: a delete that races a publish is refused, not reported as done" {
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

    // Simulate the race the handler's pre-check cannot close: the row is published
    // between "is it a draft?" and the DELETE. The statement is guarded, so it
    // matches zero rows — and a handler that ignored its RETURNING would answer 204
    // while the fleet stayed live and installable in every workspace.
    const pub_res = try publish(h, alloc, PROBE_ID);
    defer pub_res.deinit();
    try pub_res.expectStatus(.ok);

    const url = try entryUrl(alloc, PROBE_ID);
    defer alloc.free(url);
    const refused = try (try h.delete(url).bearer(TOKEN_PLATFORM)).send();
    defer refused.deinit();
    try refused.expectStatus(.conflict);
    try refused.expectErrorCode("UZ-CATALOG-003");

    // The fleet survives. That is the whole point.
    try std.testing.expectEqual(@as(i64, 1), try rowCount(conn));
    try std.testing.expectEqualStrings("public", try visibilityOf(conn, PROBE_ID));
}

test "integration: a curate-and-publish patch commits together or not at all" {
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

    // One PATCH carrying BOTH the operator's copy and the publish. The two statements
    // run in one transaction, so the fleet cannot end up published with the old copy,
    // or re-described but still a draft.
    const res = try patchEntry(h, alloc, PROBE_ID,
        \\{"description":"Operator copy.","published":true}
    );
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(res.bodyContains("Operator copy."));
    try std.testing.expect(res.bodyContains("\"visibility\":\"public\""));
    try std.testing.expectEqualStrings("public", try visibilityOf(conn, PROBE_ID));
}

test "integration: patching an entry that no longer exists is a 404, not a silent 200" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try reset(conn);

    const res = try patchEntry(h, alloc, "no-such-fleet", "{\"published\":false}");
    defer res.deinit();
    try res.expectStatus(.not_found);
    try res.expectErrorCode("UZ-CATALOG-001");
}
