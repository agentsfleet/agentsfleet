//! Integration coverage for the two template onboarding routes (M103 §2):
//! scope gating, workspace ownership, skill-only (no-R2) onboard, and the
//! tenant `(workspace_id, content_hash)` dedup. Support-file fetch paths ride
//! github/template sources and are covered by the importer + github_source unit
//! tests; these exercise the upload (paste) path, which needs no network or R2.

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
// TENANT_ADMIN holds library:write (tenant tier), not platform-library:write.
const TOKEN_TENANT = scope_fixtures.TENANT_ADMIN;
// PLATFORM_ADMIN holds platform-library:write, not library:write.
const TOKEN_PLATFORM = scope_fixtures.PLATFORM_ADMIN;

const PROBE_NAME = "onboard-probe";
const PROBE_SKILL =
    \\---
    \\name: onboard-probe
    \\description: Probe template for onboarding tests.
    \\version: 0.1.0
    \\---
    \\Body for the onboarding probe.
;

const PLATFORM_URL = "/v1/admin/fleet-libraries";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn resetAndSeed(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.tenant_fleet_library WHERE workspace_id = $1::uuid", .{http_auth.WS_PRIMARY});
    _ = try conn.exec("DELETE FROM core.tenant_fleet_library WHERE workspace_id = $1::uuid", .{http_auth.WS_SECONDARY});
    _ = try conn.exec("DELETE FROM core.fleet_library WHERE id = $1", .{PROBE_NAME});
    // Restore the migration-seeded platform rows to their un-onboarded
    // (metadata-only) state, so a test that onboards one — setting content_hash,
    // which makes it gallery-visible — never leaks into the next test.
    _ = try conn.exec("UPDATE core.fleet_library SET content_hash = NULL, skill_markdown = NULL, trigger_markdown = NULL", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

// Onboarding the github-pr-reviewer slug UPSERTs the seed row (id == the parsed
// SKILL name), setting content_hash (installable → gallery-visible) while the
// UPSERT preserves the seed's curated required_credentials_reasons.
const GH_REVIEWER_SKILL =
    \\---
    \\name: github-pr-reviewer
    \\description: Reviews GitHub pull requests.
    \\version: 0.1.0
    \\---
    \\Body for the github-pr-reviewer onboarding.
;

// Onboard `skill` into the platform tier (upload kind — no fetch, no R2).
fn onboardPlatform(h: *TestHarness, alloc: std.mem.Allocator, skill: []const u8) !void {
    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = "unit/platform",
        .skill_markdown = skill,
    }, .{});
    defer alloc.free(body);
    const res = try (try (try h.post(PLATFORM_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
    defer res.deinit();
    try res.expectStatus(.created);
}

fn tenantUrl(alloc: std.mem.Allocator, workspace_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleet-libraries", .{workspace_id});
}

/// Upload (paste) onboarding body — skill-only, no support files, so no fetch
/// and no R2 object are needed.
fn onboardBody(alloc: std.mem.Allocator) ![]const u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = "unit/onboard-probe",
        .skill_markdown = PROBE_SKILL,
    }, .{});
}

fn platformCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::bigint FROM core.fleet_library WHERE id = $1
    , .{PROBE_NAME}));
    defer q.deinit();
    const row = try q.next() orelse return error.CountMissing;
    return try row.get(i64, 0);
}

fn tenantCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::bigint FROM core.tenant_fleet_library WHERE workspace_id = $1::uuid
    , .{http_auth.WS_PRIMARY}));
    defer q.deinit();
    const row = try q.next() orelse return error.CountMissing;
    return try row.get(i64, 0);
}

test "integration: platform onboard requires platform-library:write" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const body = try onboardBody(alloc);
    defer alloc.free(body);

    // TENANT_ADMIN lacks platform-library:write → 403, nothing written.
    const denied = try (try (try h.post(PLATFORM_URL).bearer(TOKEN_TENANT)).json(body)).send();
    defer denied.deinit();
    try denied.expectStatus(.forbidden);
    try std.testing.expectEqual(@as(i64, 0), try platformCount(conn));

    // PLATFORM_ADMIN holds the scope → 201, row persisted, response tier "platform".
    const ok = try (try (try h.post(PLATFORM_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
    defer ok.deinit();
    try ok.expectStatus(.created);
    try std.testing.expect(ok.bodyContains("\"visibility\":\"platform\""));
    try std.testing.expect(ok.bodyContains("\"content_hash\""));
    try std.testing.expect(!ok.bodyContains("snapshot_key"));
    try std.testing.expectEqual(@as(i64, 1), try platformCount(conn));
}

test "integration: tenant onboard requires library:write plus workspace ownership" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const body = try onboardBody(alloc);
    defer alloc.free(body);
    const owned_url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(owned_url);

    // PLATFORM_ADMIN lacks library:write → 403 even with workspace:any.
    const no_scope = try (try (try h.post(owned_url).bearer(TOKEN_PLATFORM)).json(body)).send();
    defer no_scope.deinit();
    try no_scope.expectStatus(.forbidden);

    // TENANT_ADMIN holds library:write but does not own WS_ABSENT → 403.
    const foreign_url = try tenantUrl(alloc, http_auth.WS_ABSENT);
    defer alloc.free(foreign_url);
    const not_owned = try (try (try h.post(foreign_url).bearer(TOKEN_TENANT)).json(body)).send();
    defer not_owned.deinit();
    try not_owned.expectStatus(.forbidden);
    try std.testing.expectEqual(@as(i64, 0), try tenantCount(conn));

    // TENANT_ADMIN owns WS_PRIMARY → 201, row written under that workspace.
    const ok = try (try (try h.post(owned_url).bearer(TOKEN_TENANT)).json(body)).send();
    defer ok.deinit();
    try ok.expectStatus(.created);
    try std.testing.expect(ok.bodyContains("\"visibility\":\"tenant\""));
    try std.testing.expectEqual(@as(i64, 1), try tenantCount(conn));
}

test "integration: skill-only template onboards without an R2 object" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const body = try onboardBody(alloc);
    defer alloc.free(body);
    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);

    // The harness configures no R2 client; a skill-only onboard must still succeed
    // (no support files → no snapshot put). The stored manifest is an empty array.
    const ok = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(body)).send();
    defer ok.deinit();
    try ok.expectStatus(.created);

    var q = PgQuery.from(try conn.query(
        \\SELECT support_files_json::text FROM core.tenant_fleet_library
        \\WHERE workspace_id = $1::uuid
    , .{http_auth.WS_PRIMARY}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    try std.testing.expectEqualStrings("[]", try row.get([]const u8, 0));
}

test "integration: tenant onboard dedupes by workspace and content hash" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const body = try onboardBody(alloc);
    defer alloc.free(body);
    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);

    const first = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(body)).send();
    defer first.deinit();
    try first.expectStatus(.created);
    const first_id = try jsonStringField(alloc, first.body, "id");
    defer alloc.free(first_id);

    const second = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(body)).send();
    defer second.deinit();
    try second.expectStatus(.created);
    const second_id = try jsonStringField(alloc, second.body, "id");
    defer alloc.free(second_id);

    // Identical bytes converge on one (workspace_id, content_hash) row.
    try std.testing.expectEqualStrings(first_id, second_id);
    try std.testing.expectEqual(@as(i64, 1), try tenantCount(conn));
}

fn jsonStringField(alloc: std.mem.Allocator, body: []const u8, field: []const u8) ![]const u8 {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();
    const value = parsed.value.object.get(field) orelse return error.JsonFieldMissing;
    return switch (value) {
        .string => |s| alloc.dupe(u8, s),
        else => error.JsonFieldWrongType,
    };
}

// A tenant template row planted directly under another workspace, used to prove
// the gallery never leaks across workspaces (Dimension 5.2).
const FOREIGN_TEMPLATE_NAME = "foreign-workspace-template";

fn seedForeignTenantTemplate(conn: *pg.Conn) !void {
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_SECONDARY);
    _ = try conn.exec(
        \\INSERT INTO core.tenant_fleet_library
        \\  (id, workspace_id, name, description, source_kind, source_ref, visibility,
        \\   content_hash, skill_markdown, trigger_markdown, support_files_json,
        \\   requirements_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-0000000000d1'::uuid, $1::uuid, $2,
        \\        'foreign workspace template', 'upload', 'unit/foreign', 'tenant',
        \\        'deadbeef', 'skill', NULL, '[]'::jsonb,
        \\        '{"credentials":[],"tools":[],"network_hosts":[],"support_files":[],"trigger_present":false}'::jsonb,
        \\        0, 0)
    , .{ http_auth.WS_SECONDARY, FOREIGN_TEMPLATE_NAME });
}

test "integration: gallery unions platform and own tenant templates" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // Onboard a platform template so an installable platform row exists — raw
    // migration seeds carry no content_hash and are hidden by the gallery filter.
    try onboardPlatform(h, alloc, PROBE_SKILL);

    // Onboard one tenant template into WS_PRIMARY.
    const body = try onboardBody(alloc);
    defer alloc.free(body);
    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);
    const created = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(body)).send();
    defer created.deinit();
    try created.expectStatus(.created);

    // The gallery returns the onboarded platform template plus WS_PRIMARY's own
    // tenant template — both surface under the shared `onboard-probe` id.
    const gallery = try (try h.get(url).bearer(TOKEN_TENANT)).send();
    defer gallery.deinit();
    try gallery.expectStatus(.ok);
    try std.testing.expect(gallery.bodyContains("\"onboard-probe\"")); // onboarded platform + own tenant
    try std.testing.expect(gallery.bodyContains("\"visibility\":\"platform\""));
    try std.testing.expect(gallery.bodyContains("\"visibility\":\"tenant\""));
    // An un-onboarded migration seed (no content_hash) stays hidden until onboarded.
    try std.testing.expect(!gallery.bodyContains("\"security-reviewer\""));
    // No object-store key escapes the gallery (Dimension 5.3).
    try std.testing.expect(!gallery.bodyContains("snapshot_key"));
    try std.testing.expect(!gallery.bodyContains("fleet-bundles/sha256/"));
}

test "integration: gallery entries carry description and credential reasons" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // Onboard the github-pr-reviewer seed so it becomes installable (gallery-
    // visible). The platform UPSERT sets content_hash but preserves the seed's
    // curated required_credentials_reasons — that's what surfaces below.
    try onboardPlatform(h, alloc, GH_REVIEWER_SKILL);

    // Onboard a tenant template so the gallery exercises both tiers.
    const body = try onboardBody(alloc);
    defer alloc.free(body);
    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);
    const created = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(body)).send();
    defer created.deinit();
    try created.expectStatus(.created);

    const gallery = try (try h.get(url).bearer(TOKEN_TENANT)).send();
    defer gallery.deinit();
    try gallery.expectStatus(.ok);
    // Every entry carries the description + reasons keys (Dimension 5.4).
    try std.testing.expect(gallery.bodyContains("\"description\""));
    try std.testing.expect(gallery.bodyContains("\"required_credentials_reasons\""));
    // The onboarded platform seed surfaces its curated per-credential reason copy...
    try std.testing.expect(gallery.bodyContains("review your pull requests and post review comments"));
    // ...and the onboarded tenant template surfaces its SKILL.md description.
    try std.testing.expect(gallery.bodyContains("Probe template for onboarding tests."));
}

test "integration: gallery isolates another workspace's tenant templates" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);
    try seedForeignTenantTemplate(conn);

    // An installable platform row must survive the workspace filter unchanged.
    try onboardPlatform(h, alloc, PROBE_SKILL);

    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);
    const gallery = try (try h.get(url).bearer(TOKEN_TENANT)).send();
    defer gallery.deinit();
    try gallery.expectStatus(.ok);
    // WS_PRIMARY's gallery must not surface WS_SECONDARY's tenant template.
    try std.testing.expect(!gallery.bodyContains(FOREIGN_TEMPLATE_NAME));
    try std.testing.expect(gallery.bodyContains("\"onboard-probe\"")); // platform still shown
}
