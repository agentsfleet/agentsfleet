const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const http_auth = @import("../../../db/test_fixtures_http_auth.zig");
const importer = @import("../../../fleet_bundle/importer.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"310oH7ahxoKws6fEKmbOP30dQaQhT21HGRxvibeBuqfywkNxJ0xcfhhao1mwbLH7BUOg2GYXDEA6EvcVlKXqGN_Wa_4Q7UenmZqeXYdB_IhAc-SzyoW9hRi01FskVVI8w_N0Pf5SItu7DIqdxbKP8_eGFyrTL1mN-5klkIDCSnhrDLUEgjVo7iod0vsoqUEH-2m1s-2xDh5aQr5rSF6neCTA1-JvKVkJLD6eOdBnEwYBm6-yZ0CNgMfw1uUyw5cGwdaPsCerHctH0EwcI_qQFUUnFjBeN4FJkP_DDoHWTEV9a-5wzomOcoKlyfZvRgplGYYqTWrIAfcZobyzYiSy1w","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJ1c2VyIn19.aSqdpbu-D-1NmzJgcw-7LUJYImlFu-gbrO3fBPlMI6DFvgSGJJg3wAYe5DKJXe5ytCActeAHN8LxGyr1emB4ReHk90B7t_DB301cl5fz6H1EIBnUYkuOYIeCQXvqTmEHduR1KPumEYc6Jfw3kv1tY95k-bugObZ4FihLhWXw4ud8fXRl_CTnD3J3FSx-cn4K8mfy8JjTc1RDmEx5_4-TbBhPyTgj5EAXqB1ddUw7k46UAh_-w2G07SrOxsl1b57Etwp0gvuu4tkpXICYmG423n-RjVvtvuxjSzQyhUZ2Lmfbvi1tLlY7_uzTh_BwwWWYLdJtnmKEblmGReoAu_Qs6A";

const GITHUB_SKILL =
    \\---
    \\name: github-pr-reviewer
    \\description: Reviews GitHub pull requests.
    \\version: 0.1.0
    \\---
    \\Review pull request context and return concise review comments.
;

const GITHUB_TRIGGER =
    \\---
    \\name: github-pr-reviewer
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: webhook
    \\      source: github
    \\  credentials: [github]
    \\  tools: [github_review_comment]
    \\  network:
    \\    allow: [api.github.com]
    \\  budget:
    \\    daily_dollars: 1.0
    \\---
;

const INSTALL_SKILL =
    \\---
    \\name: bundle-install-pin
    \\description: Installs from a stored bundle.
    \\version: 0.1.0
    \\---
    \\Use this bundle to prove bundle_id install.
;

const INSTALL_TRIGGER =
    \\---
    \\name: bundle-install-pin
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: api
    \\  tools: []
    \\  budget:
    \\    daily_dollars: 1.0
    \\---
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

fn resetAndSeed(conn: *pg.Conn) !void {
    _ = try conn.exec("DELETE FROM core.fleet_events WHERE workspace_id = $1::uuid", .{http_auth.WS_PRIMARY});
    _ = try conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid", .{http_auth.WS_PRIMARY});
    _ = try conn.exec("DELETE FROM core.fleet_bundles WHERE workspace_id = $1::uuid", .{http_auth.WS_PRIMARY});
    _ = try conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{http_auth.WS_PRIMARY});
    http_auth.cleanup(conn);
    try http_auth.seedTenant(conn);
    try http_auth.seedScopeWorkspace(conn, http_auth.WS_PRIMARY);
}

fn importUrl(alloc: std.mem.Allocator) ![]const u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/bundles/snapshots", .{http_auth.WS_PRIMARY});
}

fn fleetCreateUrl(alloc: std.mem.Allocator) ![]const u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{http_auth.WS_PRIMARY});
}

fn bundleDetailUrl(alloc: std.mem.Allocator, bundle_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/bundles/snapshots/{s}", .{ http_auth.WS_PRIMARY, bundle_id });
}

fn importBundle(
    h: *TestHarness,
    source_ref: []const u8,
    skill_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    support_files: []const importer.SupportFile,
) !struct {
    bundle_id: []const u8,
    body: []const u8,
} {
    const alloc = h.alloc;
    const request_body = try std.json.Stringify.valueAlloc(alloc, importer.ImportBody{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = source_ref,
        .skill_markdown = skill_markdown,
        .trigger_markdown = trigger_markdown,
        .support_files = support_files,
    }, .{});
    defer alloc.free(request_body);

    const url = try importUrl(alloc);
    defer alloc.free(url);
    const response = try (try (try h.post(url).bearer(TOKEN_USER)).json(request_body)).send();
    defer response.deinit();
    try response.expectStatus(.created);
    return .{
        .bundle_id = try jsonStringField(alloc, response.body, "bundle_id"),
        .body = try alloc.dupe(u8, response.body),
    };
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

fn fleetBundleRow(conn: *pg.Conn, alloc: std.mem.Allocator, name: []const u8) !struct {
    bundle_id: []const u8,
    bundle_content_hash: []const u8,
    bundle_snapshot_key: []const u8,
} {
    var q = PgQuery.from(try conn.query(
        \\SELECT bundle_id::text, bundle_content_hash, bundle_snapshot_key
        \\FROM core.fleets
        \\WHERE workspace_id = $1::uuid AND name = $2
    , .{ http_auth.WS_PRIMARY, name }));
    defer q.deinit();
    const row = try q.next() orelse return error.FleetRowMissing;
    const bundle_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(bundle_id);
    const bundle_content_hash = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(bundle_content_hash);
    const bundle_snapshot_key = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(bundle_snapshot_key);
    return .{
        .bundle_id = bundle_id,
        .bundle_content_hash = bundle_content_hash,
        .bundle_snapshot_key = bundle_snapshot_key,
    };
}

fn freeFleetBundleRow(alloc: std.mem.Allocator, row: anytype) void {
    alloc.free(row.bundle_id);
    alloc.free(row.bundle_content_hash);
    alloc.free(row.bundle_snapshot_key);
}

fn fleetCountByName(conn: *pg.Conn, name: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::bigint
        \\FROM core.fleets
        \\WHERE workspace_id = $1::uuid AND name = $2
    , .{ http_auth.WS_PRIMARY, name }));
    defer q.deinit();
    const row = try q.next() orelse return error.CountMissing;
    return try row.get(i64, 0);
}

test "integration: Fleet Bundle import persists metadata and detail preview" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // Paste/upload carries SKILL + TRIGGER only; support files ride GitHub/template
    // sources, exercised by the github_source extraction tests + resolve unit tests.
    const imported = try importBundle(h, "unit/github-pr-reviewer", GITHUB_SKILL, GITHUB_TRIGGER, &.{});
    defer alloc.free(imported.bundle_id);
    defer alloc.free(imported.body);

    try std.testing.expect(std.mem.indexOf(u8, imported.body, "\"github\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, imported.body, "\"api.github.com\"") != null);

    const detail_url = try bundleDetailUrl(alloc, imported.bundle_id);
    defer alloc.free(detail_url);
    const detail = try (try h.get(detail_url).bearer(TOKEN_USER)).send();
    defer detail.deinit();
    try detail.expectStatus(.ok);
    try std.testing.expect(detail.bodyContains("\"github-pr-reviewer\""));
    try std.testing.expect(detail.bodyContains("\"api.github.com\""));

    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::bigint
        \\FROM core.fleet_bundles
        \\WHERE workspace_id = $1::uuid
        \\  AND id = $2::uuid
        \\  AND content_hash <> ''
        \\  AND snapshot_key LIKE 'fleet-bundles/sha256/%'
    , .{ http_auth.WS_PRIMARY, imported.bundle_id }));
    defer q.deinit();
    const row = try q.next() orelse return error.BundleRowMissing;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

test "integration: fleet create accepts bundle_id and records source metadata" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const imported = try importBundle(h, "unit/bundle-install-pin", INSTALL_SKILL, INSTALL_TRIGGER, &.{});
    defer alloc.free(imported.bundle_id);
    defer alloc.free(imported.body);

    const install_body = try std.fmt.allocPrint(alloc, "{{\"bundle_id\":\"{s}\"}}", .{imported.bundle_id});
    defer alloc.free(install_body);
    const url = try fleetCreateUrl(alloc);
    defer alloc.free(url);
    const response = try (try (try h.post(url).bearer(TOKEN_USER)).json(install_body)).send();
    defer response.deinit();
    try response.expectStatus(.created);
    try std.testing.expect(response.bodyContains("\"fleet_id\":\""));
    try std.testing.expect(response.bodyContains("\"fleet_id\":\""));
    try std.testing.expect(response.bodyContains("\"name\":\"bundle-install-pin\""));

    const row = try fleetBundleRow(conn, alloc, "bundle-install-pin");
    defer freeFleetBundleRow(alloc, row);
    try std.testing.expectEqualStrings(imported.bundle_id, row.bundle_id);
    try std.testing.expect(row.bundle_content_hash.len > 0);
    try std.testing.expect(std.mem.startsWith(u8, row.bundle_snapshot_key, "fleet-bundles/sha256/"));
}

test "integration: fleet create bundle reports missing credentials" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    const imported = try importBundle(h, "unit/missing-credential-pin", GITHUB_SKILL, GITHUB_TRIGGER, &.{});
    defer alloc.free(imported.bundle_id);
    defer alloc.free(imported.body);

    const install_body = try std.fmt.allocPrint(alloc, "{{\"bundle_id\":\"{s}\"}}", .{imported.bundle_id});
    defer alloc.free(install_body);
    const url = try fleetCreateUrl(alloc);
    defer alloc.free(url);
    const response = try (try (try h.post(url).bearer(TOKEN_USER)).json(install_body)).send();
    defer response.deinit();

    try std.testing.expectEqual(@as(u16, 424), response.status);
    try response.expectErrorCode("UZ-BUNDLE-003");
    try std.testing.expect(response.bodyContains("\"missing_credentials\":[\"github\"]"));
    try std.testing.expectEqual(@as(i64, 0), try fleetCountByName(conn, "github-pr-reviewer"));
}

test "integration: Fleet Bundle upload rejects support files" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // Attachments are only accepted on fetched (github/template) sources; an
    // upload that carries support files is a 400 and stores nothing.
    const request_body = try std.json.Stringify.valueAlloc(alloc, importer.ImportBody{
        .source_kind = importer.SOURCE_KIND_UPLOAD,
        .source_ref = "unit/reject-support",
        .skill_markdown = GITHUB_SKILL,
        .trigger_markdown = GITHUB_TRIGGER,
        .support_files = &.{.{ .path = "README.md", .content = "x" }},
    }, .{});
    defer alloc.free(request_body);

    const url = try importUrl(alloc);
    defer alloc.free(url);
    const response = try (try (try h.post(url).bearer(TOKEN_USER)).json(request_body)).send();
    defer response.deinit();
    try std.testing.expectEqual(@as(u16, 400), response.status);
    try response.expectErrorCode("UZ-BUNDLE-001");

    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::bigint FROM core.fleet_bundles WHERE workspace_id = $1::uuid
    , .{http_auth.WS_PRIMARY}));
    defer q.deinit();
    const row = try q.next() orelse return error.CountMissing;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
}

test "integration: template catalog lists seeded first-party templates from the table" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // GET /v1/fleets/bundles serves core.fleet_bundle_templates (migration-seeded,
    // not a hardcoded Zig array). The JSONB requirement columns come back as JSON
    // arrays, not quoted JSONB text.
    const res = try (try h.get("/v1/fleets/bundles").bearer(TOKEN_USER)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(res.bodyContains("\"items\""));
    try std.testing.expect(res.bodyContains("\"github-pr-reviewer\""));
    try std.testing.expect(res.bodyContains("\"security-reviewer\""));
    try std.testing.expect(res.bodyContains("\"required_credentials\""));
    // The JSONB array decodes to a JSON array, not quoted JSONB text.
    try std.testing.expect(res.bodyContains("[\"github\"]"));
    // Per-credential reasons round-trip as a nested object (not quoted text), so
    // the install gate can render the seeded "why connect" copy.
    try std.testing.expect(res.bodyContains("\"required_credentials_reasons\""));
    try std.testing.expect(res.bodyContains("review your pull requests and post review comments"));
}

const PRIVATE_PROBE_ID = "private-visibility-probe";

test "integration: catalog hides non-public templates (visibility filter)" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try resetAndSeed(conn);

    // Seed a private-visibility row directly. The catalog filters
    // `WHERE visibility = 'public'`, so this probe must NOT surface — without
    // this negative test the filter could be dropped and every seed (all
    // public) would still pass. Probe is removed afterward so the curated set
    // is left intact.
    _ = conn.exec("DELETE FROM core.fleet_bundle_templates WHERE id = $1", .{PRIVATE_PROBE_ID}) catch {};
    _ = try conn.exec(
        \\INSERT INTO core.fleet_bundle_templates
        \\    (id, name, description, source_repo, source_path, source_ref,
        \\     required_credentials, required_tools, network_hosts, visibility,
        \\     created_at, updated_at)
        \\VALUES
        \\    ($1, 'Private probe', 'Hidden from the gallery.',
        \\     'agentsfleet/private-visibility-probe', '', 'main',
        \\     '[]'::jsonb, '[]'::jsonb, '[]'::jsonb, 'private', 0, 0)
    , .{PRIVATE_PROBE_ID});

    const res = try (try h.get("/v1/fleets/bundles").bearer(TOKEN_USER)).send();
    defer res.deinit();
    try res.expectStatus(.ok);
    try std.testing.expect(!res.bodyContains(PRIVATE_PROBE_ID)); // private hidden
    try std.testing.expect(res.bodyContains("\"github-pr-reviewer\"")); // public still shown

    _ = conn.exec("DELETE FROM core.fleet_bundle_templates WHERE id = $1", .{PRIVATE_PROBE_ID}) catch {};
}
