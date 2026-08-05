//! Integration coverage for the two template onboarding routes (M103 §2):
//! scope gating, workspace ownership, skill-only (no-R2) onboard, and the
//! tenant `(workspace_id, content_hash)` dedup. Support-file fetch paths ride
//! github/template sources and are covered by the importer + github_source unit
//! tests; these exercise the upload (paste) path, which needs no network or R2.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const scope_fixtures = @import("../../test_scope_tokens.zig");
const http_auth = @import("../../../db/test_fixtures_http_auth.zig");
const importer = @import("../../../fleet_library/importer.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
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
    // The catalog is runtime-owned (M128): no migration seeds it, so a test starts
    // from an empty table and creates exactly the rows it needs.
    _ = try conn.exec("DELETE FROM core.fleet_library", .{});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

// Onboarding the github-pr-reviewer slug UPSERTs the seed row (id == the parsed
// SKILL name), setting content_hash (installable → gallery-visible) while the
// UPSERT preserves the seed's curated required_credentials_reasons.
const GH_REVIEWER_NAME = "github-pr-reviewer";
const DRAFT_NAME = "draft-only-probe";
/// Added but never published — the gallery must never carry it.
const DRAFT_SKILL =
    \\---
    \\name: draft-only-probe
    \\description: Added, never published.
    \\version: 0.1.0
    \\---
    \\Body for the draft-only probe.
;

const GH_REVIEWER_SKILL =
    \\---
    \\name: github-pr-reviewer
    \\description: Reviews GitHub pull requests.
    \\version: 0.1.0
    \\---
    \\Body for the github-pr-reviewer onboarding.
;

/// Publish a platform entry. Onboarding stages every write as a draft (M128), and
/// a draft is invisible to tenants, so any test asserting gallery visibility must
/// publish first — which is the product behaviour, not a test artefact.
fn publishPlatform(h: *TestHarness, alloc: std.mem.Allocator, id: []const u8) !void {
    const url = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ PLATFORM_URL, id });
    defer alloc.free(url);
    const res = try (try (try h.patch(url).bearer(TOKEN_PLATFORM)).json("{\"published\":true}")).send();
    defer res.deinit();
    try res.expectStatus(.ok);
}

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

/// A crew-shaped upload: BOTH markdown bodies, the way `library/incident-*/`
/// ships. `onboardBody` above sends `SKILL.md` alone, which is the older probe
/// shape and cannot say whether the trigger participates in the content hash.
fn crewUploadBody(alloc: std.mem.Allocator, trigger: []const u8) ![]const u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = "unit/onboard-probe",
        .skill_markdown = PROBE_SKILL,
        .trigger_markdown = trigger,
    }, .{});
}

/// The smallest frontmatter that parses AND carries an access level. `triggers`,
/// `tools`, and `budget` are each required by `config_parser`; the runtime keys
/// live inside `x-agentsfleet`; and `repositories` + `repository_access` are
/// optional TOGETHER — one without the other is an authoring error, not a
/// half-binding.
const CREW_TRIGGER_READ =
    \\---
    \\name: onboard-probe
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: api
    \\  tools:
    \\    - http_request
    \\  budget:
    \\    daily_dollars: 1.0
    \\  repositories:
    \\    - acme/payments
    \\  repository_access: read
    \\---
;
/// Byte-identical to the above but for the access level — the smallest edit that
/// changes what a fleet installed from this entry may DO.
const CREW_TRIGGER_WRITE =
    \\---
    \\name: onboard-probe
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: api
    \\  tools:
    \\    - http_request
    \\  budget:
    \\    daily_dollars: 1.0
    \\  repositories:
    \\    - acme/payments
    \\  repository_access: write
    \\---
;

test "test_upload_is_content_addressed" {
    // Dimension 4a.3. Re-uploading identical markdown converges on one entry, so
    // a crew applied twice from a checkout does not accumulate rows.
    //
    // The load-bearing half is the SECOND assertion: the trigger has to
    // participate in the hash. Both crew bundles are onboarded into one
    // workspace and differ in their TRIGGER.md far more than in their SKILL.md —
    // and `repository_access` lives there. If only the skill were hashed, an
    // entry could be re-uploaded with `read` silently swapped to `write` and
    // dedupe to the row that was already published.
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);

    const read_body = try crewUploadBody(alloc, CREW_TRIGGER_READ);
    defer alloc.free(read_body);

    const first = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(read_body)).send();
    defer first.deinit();
    try first.expectStatus(.created);
    const first_id = try jsonStringField(alloc, first.body, "id");
    defer alloc.free(first_id);

    // Identical bytes, again: content-addressed onto the same row.
    const again = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(read_body)).send();
    defer again.deinit();
    try again.expectStatus(.created);
    const again_id = try jsonStringField(alloc, again.body, "id");
    defer alloc.free(again_id);

    try std.testing.expectEqualStrings(first_id, again_id);
    try std.testing.expectEqual(@as(i64, 1), try tenantCount(conn));

    // Same skill, different trigger — a DIFFERENT entry. Converging here would
    // mean the access level a bundle declares is outside its identity.
    const write_body = try crewUploadBody(alloc, CREW_TRIGGER_WRITE);
    defer alloc.free(write_body);

    const escalated = try (try (try h.post(url).bearer(TOKEN_TENANT)).json(write_body)).send();
    defer escalated.deinit();
    try escalated.expectStatus(.created);
    const escalated_id = try jsonStringField(alloc, escalated.body, "id");
    defer alloc.free(escalated_id);

    try std.testing.expect(!std.mem.eql(u8, first_id, escalated_id));
    try std.testing.expectEqual(@as(i64, 2), try tenantCount(conn));
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

    // Add a platform fleet AND publish it: an unpublished entry is invisible to
    // every tenant, which is the whole point of the publish gate.
    try onboardPlatform(h, alloc, PROBE_SKILL);
    try publishPlatform(h, alloc, PROBE_NAME);

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
    // A DRAFT platform entry stays hidden. Added, never published — so the gallery
    // must not carry it, and the claim stays load-bearing.
    try onboardPlatform(h, alloc, DRAFT_SKILL);
    const after_draft = try (try h.get(url).bearer(TOKEN_TENANT)).send();
    defer after_draft.deinit();
    try std.testing.expect(!after_draft.bodyContains(DRAFT_NAME));
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

    // Add github-pr-reviewer, write the curated per-credential copy an operator
    // would write (no bundle can supply it), then publish. That copy is what the
    // install gate shows a tenant, and it is what must surface below.
    try onboardPlatform(h, alloc, GH_REVIEWER_SKILL);
    const curate_url = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ PLATFORM_URL, GH_REVIEWER_NAME });
    defer alloc.free(curate_url);
    const curated = try (try (try h.patch(curate_url).bearer(TOKEN_PLATFORM)).json(
        \\{"required_credentials_reasons":{"github":"review your pull requests and post review comments"}}
    )).send();
    defer curated.deinit();
    try curated.expectStatus(.ok);
    try publishPlatform(h, alloc, GH_REVIEWER_NAME);

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

    // A published platform row must survive the workspace filter unchanged.
    try onboardPlatform(h, alloc, PROBE_SKILL);
    try publishPlatform(h, alloc, PROBE_NAME);

    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);
    const gallery = try (try h.get(url).bearer(TOKEN_TENANT)).send();
    defer gallery.deinit();
    try gallery.expectStatus(.ok);
    // WS_PRIMARY's gallery must not surface WS_SECONDARY's tenant template.
    try std.testing.expect(!gallery.bodyContains(FOREIGN_TEMPLATE_NAME));
    try std.testing.expect(gallery.bodyContains("\"onboard-probe\"")); // platform still shown
}

test "test_import_manifest_survives_store_round_trip" {
    // The manifest's round-trip through the REAL insert. The unit pin proves
    // the statement TEXT still names support_files_json; this proves the
    // BINDING does — a mis-bound or dropped parameter would persist '[]' or
    // NULL while every substring pin stayed green.
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const body = importer.ImportBody{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "unit/manifest-roundtrip",
        .skill_markdown = "---\nname: manifest-roundtrip\ndescription: d\nversion: 0.1.0\n---\nBody.\n",
        .support_files = &.{.{ .path = "docs/NOTES.md", .content = "kept" }},
    };
    const prepared = try importer.prepare(alloc, body);
    defer prepared.deinit(alloc);

    const id = try library_store.insertOrFetchTenant(conn, alloc, .{
        .id = "0195b4ba-8d3a-7f13-8abc-00000000d1aa",
        .workspace_id = http_auth.WS_PRIMARY,
        .name = prepared.name,
        .description = "manifest round trip",
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "unit/manifest-roundtrip",
        .content_hash = prepared.content_hash,
        .skill_markdown = body.skill_markdown,
        .trigger_markdown = null,
        .support_files_json = prepared.support_files_json,
        .requirements_json = prepared.requirements_json,
        .now_ms = 1,
    });
    defer alloc.free(id);

    var q = PgQuery.from(try conn.query(
        \\SELECT support_files_json::text FROM core.tenant_fleet_library WHERE id = $1::uuid
    , .{id}));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    const stored = try row.get([]const u8, 0);
    try std.testing.expect(std.mem.indexOf(u8, stored, "docs/NOTES.md") != null);
    try std.testing.expect(std.mem.indexOf(u8, stored, "sha256") != null);
}

// ── Dimension 5.3 — the SHIPPED crew reaches a workspace ────────────────────

const LIBRARY_BASE = "library";
const CREW_SLUGS = [_][]const u8{ "incident-responder", "incident-repairer" };
const MAX_BUNDLE_BYTES = 64 * 1024;

fn loadBundleFile(alloc: std.mem.Allocator, slug: []const u8, file: []const u8) ![]u8 {
    const path = try std.fs.path.join(alloc, &.{ LIBRARY_BASE, slug, file });
    defer alloc.free(path);
    return std.Io.Dir.cwd().readFileAlloc(common.globalIo(), path, alloc, .limited(MAX_BUNDLE_BYTES));
}

/// Onboard one shipped bundle into the platform tier through the real route,
/// carrying BOTH markdown bodies exactly as they sit on disk.
fn onboardCrewBundle(h: *TestHarness, alloc: std.mem.Allocator, slug: []const u8) !void {
    const skill = try loadBundleFile(alloc, slug, "SKILL.md");
    defer alloc.free(skill);
    const trigger = try loadBundleFile(alloc, slug, "TRIGGER.md");
    defer alloc.free(trigger);

    const body = try std.json.Stringify.valueAlloc(alloc, .{
        .source_kind = "upload",
        .source_ref = "library/crew",
        .skill_markdown = skill,
        .trigger_markdown = trigger,
    }, .{});
    defer alloc.free(body);

    const res = try (try (try h.post(PLATFORM_URL).bearer(TOKEN_PLATFORM)).json(body)).send();
    defer res.deinit();
    try res.expectStatus(.created);
}

test "test_bundles_publish_and_list" {
    // Dimension 5.3. Onboard → publish → visible → installable, for the bundles
    // this milestone actually ships, through the existing admin flow.
    //
    // It is the only test that drives the shipped markdown through the IMPORTER
    // rather than the config parser, and the two demand different things. The
    // importer needs `SKILL.md` frontmatter for the entry's name, and it demands
    // that name match `TRIGGER.md`'s — so a bundle can parse perfectly as a fleet
    // config and still be impossible to install, which is exactly the state the
    // repairer shipped in until this Dimension was built.
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    for (CREW_SLUGS) |slug| {
        try onboardCrewBundle(h, alloc, slug);
        // A draft is invisible to every tenant, so publication is the step that
        // makes a crew reachable — not onboarding.
        try publishPlatform(h, alloc, slug);
    }

    const url = try tenantUrl(alloc, http_auth.WS_PRIMARY);
    defer alloc.free(url);
    const gallery = try (try h.get(url).bearer(TOKEN_TENANT)).send();
    defer gallery.deinit();
    try gallery.expectStatus(.ok);

    for (CREW_SLUGS) |slug| {
        const quoted = try std.fmt.allocPrint(alloc, "\"{s}\"", .{slug});
        defer alloc.free(quoted);
        try std.testing.expect(gallery.bodyContains(quoted));
    }
    // Installable, not merely present: a draft is invisible to every tenant and
    // uninstallable by id, so surfacing at platform visibility IS the reachable
    // state — publication is what a tenant can act on.
    try std.testing.expect(gallery.bodyContains("\"visibility\":\"platform\""));

    // The requirements the gallery advertises are derived from the SHIPPED
    // TRIGGER.md, so an operator sees what each member will ask for before
    // installing it — the repairer's write reach included.
    try std.testing.expect(gallery.bodyContains("api.github.com"));
    try std.testing.expect(gallery.bodyContains("repo_fetch"));
}
