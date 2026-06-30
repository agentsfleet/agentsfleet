// HTTP integration tests for the fleets CRUD API — focused on cursor
// pagination on GET /v1/workspaces/{ws}/fleets.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const id_format = @import("../../../types/id_format.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_USER = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedWorkspace(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'ListPaginationTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
}

fn seedFleets(alloc: std.mem.Allocator, conn: *pg.Conn, count: usize, base_ms: i64) ![][]const u8 {
    var ids = try alloc.alloc([]const u8, count);
    errdefer {
        for (ids[0..]) |id| if (id.len > 0) alloc.free(id);
        alloc.free(ids);
    }
    for (0..count) |i| {
        const id = try id_format.generateFleetId(alloc);
        ids[i] = id;
        const name = try std.fmt.allocPrint(alloc, "fleet-pg-{d}-{d}", .{ base_ms, i });
        defer alloc.free(name);
        _ = try conn.exec(
            \\INSERT INTO core.fleets
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, '{}'::jsonb, 'active', $4, $4)
        , .{ id, TEST_WORKSPACE_ID, name, base_ms + @as(i64, @intCast(i)) });
    }
    return ids;
}

fn freeIds(alloc: std.mem.Allocator, ids: [][]const u8) void {
    for (ids) |id| alloc.free(id);
    alloc.free(ids);
}

// ── Cursor pagination roundtrip + invalid-cursor handling ────────────────────

test "integration: fleets list — cursor pagination roundtrip" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);
    const ids = try seedFleets(alloc, conn, 5, now_ms);
    defer freeIds(alloc, ids);

    // Full cursor round-trip: 5 fleets seeded, limit=2 means pages of
    // 2 + 2 + 1. Walk every page, accumulate ids, and assert:
    //   (a) continuation has no overlap with prior pages,
    //   (b) last page carries cursor=null,
    //   (c) union of ids across pages == seeded set (order agnostic).
    var seen_ids = std.StringHashMap(void).init(alloc);
    defer {
        // Free every duped id key on ALL exit paths — an early return on a failed
        // assertion must not leak the keys accumulated so far.
        var key_it = seen_ids.keyIterator();
        while (key_it.next()) |key_ptr| alloc.free(key_ptr.*);
        seen_ids.deinit();
    }

    var next_cursor: ?[]const u8 = null;
    var page_count: usize = 0;
    while (page_count < 10) : (page_count += 1) { // hard cap guards runaway loop
        const url = if (next_cursor) |c|
            try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets?limit=2&cursor={s}", .{ TEST_WORKSPACE_ID, c })
        else
            try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets?limit=2", .{TEST_WORKSPACE_ID});
        defer alloc.free(url);
        if (next_cursor) |c| alloc.free(c);

        const r = try (try h.get(url).bearer(TOKEN_USER)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        const parsed = try std.json.parseFromSlice(std.json.Value, alloc, r.body, .{});
        defer parsed.deinit();

        const items = parsed.value.object.get("items").?.array;
        for (items.items) |item| {
            const id = item.object.get("id").?.string;
            const id_copy = try alloc.dupe(u8, id);
            const gop = try seen_ids.getOrPut(id_copy);
            try std.testing.expect(!gop.found_existing); // (a) no overlap across pages
        }

        const cursor_node = parsed.value.object.get("cursor").?;
        switch (cursor_node) {
            .null => {
                next_cursor = null;
                break; // (b) terminal page reached
            },
            .string => |s| next_cursor = try alloc.dupe(u8, s),
            else => return error.UnexpectedCursorType,
        }
    }
    try std.testing.expect(next_cursor == null);
    // (c) every seeded fleet was returned across the walk. This workspace is
    // shared across integration tests under the parallel runner, so assert the
    // seeded set is a SUBSET of what we saw — not an exact count. Pagination
    // correctness (no cross-page overlap, terminal cursor) is what this test owns.
    for (ids) |seeded| try std.testing.expect(seen_ids.contains(seeded));

    // Bad cursor → 400.
    const url_bad = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets?cursor=not-a-cursor", .{TEST_WORKSPACE_ID});
    defer alloc.free(url_bad);
    const r_bad = try (try h.get(url_bad).bearer(TOKEN_USER)).send();
    defer r_bad.deinit();
    try r_bad.expectStatus(.bad_request);

    // No-token → 401.
    const url_anon = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url_anon);
    const r_anon = try h.get(url_anon).send();
    defer r_anon.deinit();
    try r_anon.expectStatus(.unauthorized);
}

test "integration: fleets list — projects triggers array from config_json" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    // Seed one fleet with a `triggers[]` array inside `x-agentsfleet:` — mirrors
    // what config_parser.zig persists for a real install. Bypasses the HTTP
    // create path (Redis required) but exercises the SELECT projection that
    // list.zig adds: `config_json->'x-agentsfleet'->'triggers'`.
    const zid = try id_format.generateFleetId(alloc);
    defer alloc.free(zid);
    const config_json =
        \\{"name":"triggers-projection","x-agentsfleet":{"triggers":[
        \\  {"type":"webhook","source":"github","events":["workflow_run"]},
        \\  {"type":"cron","schedule":"*/30 * * * *"}
        \\]}}
    ;
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, $4::jsonb, 'active', $5, $5)
    , .{ zid, TEST_WORKSPACE_ID, "triggers-projection", config_json, now_ms + 9_000 });

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets?limit=20", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);

    const r = try (try h.get(url).bearer(TOKEN_USER)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    // Newest-first ordering puts our fleet at index 0 (created_at = now+9000s,
    // greater than any sibling rows from the pagination suite).
    try std.testing.expect(r.bodyContains("\"name\":\"triggers-projection\""));
    try std.testing.expect(r.bodyContains("\"type\":\"webhook\""));
    try std.testing.expect(r.bodyContains("\"source\":\"github\""));
    try std.testing.expect(r.bodyContains("\"workflow_run\""));
    try std.testing.expect(r.bodyContains("\"type\":\"cron\""));
    try std.testing.expect(r.bodyContains("\"schedule\":\"*/30 * * * *\""));
}

// list.zig projects two per-fleet aggregates the `agentsfleet status` table
// renders: events_processed (COUNT of core.fleet_events) and budget_used_nanos
// (SUM of fleet_execution_telemetry.credit_deducted_nanos). Seed 3 events + 2
// telemetry rows for one fleet and assert the list reflects 3 / 3_000_000.
test "integration: fleets list — projects events_processed and budget_used_nanos aggregates" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    // created_at far in the future so newest-first ordering puts this fleet on
    // page 1 regardless of rows left by sibling tests.
    const zid = try id_format.generateFleetId(alloc);
    defer alloc.free(zid);
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, '{}'::jsonb, 'active', $4, $4)
    , .{ zid, TEST_WORKSPACE_ID, "aggregates-fleet", now_ms + 20_000 });

    // Telemetry is tenant-scoped with no FK cascade; clean up the rows this
    // test seeds (and the fleet, which cascades its events) so the shared test
    // tenant stays telemetry-free for the billing "no telemetry" suite.
    defer {
        _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE fleet_id = $1", .{zid}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
        _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{zid}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    }

    // 3 events → events_processed = 3.
    for (0..3) |i| {
        const uid = try id_format.generateFleetId(alloc);
        defer alloc.free(uid);
        const event_id = try std.fmt.allocPrint(alloc, "evt-agg-{d}", .{i});
        defer alloc.free(event_id);
        _ = try conn.exec(
            \\INSERT INTO core.fleet_events
            \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status, request_json, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, 'webhook:test', 'webhook', 'done', '{}'::jsonb, $5, $5)
        , .{ uid, zid, event_id, TEST_WORKSPACE_ID, now_ms });
    }

    // 2 telemetry rows → budget_used_nanos = 1_000_000 + 2_000_000 = 3_000_000.
    // (telemetry workspace_id / fleet_id columns are TEXT, not uuid.)
    const charges = [_]i64{ 1_000_000, 2_000_000 };
    for (charges, 0..) |nanos, i| {
        const uid = try id_format.generateFleetId(alloc);
        defer alloc.free(uid);
        const tid = try std.fmt.allocPrint(alloc, "tel-agg-{d}", .{i});
        defer alloc.free(tid);
        const event_id = try std.fmt.allocPrint(alloc, "tel-evt-agg-{d}", .{i});
        defer alloc.free(event_id);
        _ = try conn.exec(
            \\INSERT INTO core.fleet_execution_telemetry
            \\  (uid, id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture, model, credit_deducted_nanos, recorded_at)
            \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, 'stage', 'platform', 'claude', $7, $8)
        , .{ uid, tid, TEST_TENANT_ID, TEST_WORKSPACE_ID, zid, event_id, nanos, now_ms });
    }

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets?limit=50", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);

    const r = try (try h.get(url).bearer(TOKEN_USER)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    try std.testing.expect(r.bodyContains("\"name\":\"aggregates-fleet\""));
    try std.testing.expect(r.bodyContains("\"events_processed\":3"));
    try std.testing.expect(r.bodyContains("\"budget_used_nanos\":3000000"));
}

// §2 rule: 201 install response carries a `webhook_urls` map keyed by
// `triggers[].source`. URL pattern is `{api_url}/v1/webhooks/{id}/{source}`.
// The CLI install-skill consumes this map verbatim when looping `gh api`
// per declared webhook trigger — a wrong value (or missing field) drops the
// install skill into the paste-into-GitHub fallback the spec eliminates.
test "integration: install — 201 returns webhook_urls map keyed by source" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    const body =
        "{\"source_markdown\":\"---\\nname: webhook-install-pin\\ndescription: pins webhook_urls shape\\nversion: 0.1.0\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: webhook-install-pin\\nx-agentsfleet:\\n  triggers:\\n    - type: webhook\\n      source: github\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    try std.testing.expect(r.bodyContains("\"webhook_urls\":{"));
    try std.testing.expect(r.bodyContains("\"github\":\"http://127.0.0.1/v1/webhooks/"));
    try std.testing.expect(r.bodyContains("/github\""));
}

// §Failure Modes row: "Install with no webhook trigger → 201 with
// `webhook_urls: {}`". A cron-only fleet short-circuits the install-skill's
// S1.9 `gh api` loop — the empty map is the signal that there is nothing to
// register on the upstream side, and the skill validates via smoke-test
// steer at S1.11 instead.
test "integration: install — cron-only trigger returns empty webhook_urls map" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    const body =
        "{\"source_markdown\":\"---\\nname: cron-only-install-pin\\ndescription: pins empty webhook_urls\\nversion: 0.1.0\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: cron-only-install-pin\\nx-agentsfleet:\\n  triggers:\\n    - type: cron\\n      schedule: '*/30 * * * *'\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    try std.testing.expect(r.bodyContains("\"webhook_urls\":{}"));
}

test "integration: install — SKILL.md-only body generates default API trigger" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    const body =
        "{\"source_markdown\":\"---\\nname: skill-only-install-pin\\ndescription: pins default trigger generation\\nversion: 0.1.0\\n---\\nBody.\\n\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);
    try std.testing.expect(r.bodyContains("\"name\":\"skill-only-install-pin\""));
    try std.testing.expect(r.bodyContains("\"webhook_urls\":{}"));

    var q = PgQuery.from(try conn.query(
        "SELECT trigger_markdown, config_json #>> '{x-agentsfleet,triggers,0,type}' FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2",
        .{ TEST_WORKSPACE_ID, "skill-only-install-pin" },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.FleetRowMissing;
    const trigger_markdown = (try row.get(?[]const u8, 0)) orelse return error.TriggerMissing;
    const trigger_type = (try row.get(?[]const u8, 1)) orelse return error.TriggerTypeMissing;
    try std.testing.expect(std.mem.indexOf(u8, trigger_markdown, "name: skill-only-install-pin") != null);
    try std.testing.expect(std.mem.indexOf(u8, trigger_markdown, "type: api") != null);
    try std.testing.expect(std.mem.indexOf(u8, trigger_markdown, "tools: []") != null);
    try std.testing.expectEqualStrings("api", trigger_type);
}

// Cross-file `name:` invariant: SKILL.md and TRIGGER.md must agree on identity.
// Handler enforcement at create.zig fires before workspace authorization, so a
// USER-role token still surfaces the mismatch error (no escalation needed).
test "integration: fleet create rejects SKILL/TRIGGER name mismatch with UZ-AGT-011" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    // SKILL.md says alpha-fleet; TRIGGER.md says beta-fleet. Both halves
    // parse cleanly in isolation — the rejection only fires at the install
    // handler, which is what this test pins.
    const body =
        "{\"source_markdown\":\"---\\nname: alpha-fleet\\ndescription: alpha\\nversion: 0.1.0\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: beta-fleet\\nx-agentsfleet:\\n  triggers:\\n    - type: webhook\\n      source: agentmail\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-AGT-011");
}

/// Reads the stored required_tags for a fleet (by name) as a comma-joined
/// string so the assertion is order-explicit. Row-backed slice is compared
/// in-function before deinit.
fn requiredTagsCsv(conn: *pg.Conn, alloc: std.mem.Allocator, name: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query(
        "SELECT array_to_string(ARRAY(SELECT unnest(required_tags) ORDER BY 1), ',') FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2",
        .{ TEST_WORKSPACE_ID, name },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.FleetRowMissing;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

// Spec Dimension 1.1: a fleet persists required_tags from create. The placement
// eligibility suite seeds tags via a raw UPDATE; THIS test proves the create
// handler actually writes the SKILL.md frontmatter `tags:` into the column —
// the S3 persistence path, end-to-end through the real router.
test "integration: fleet create persists SKILL.md tags into required_tags" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    const body =
        "{\"source_markdown\":\"---\\nname: tag-persist-pin\\ndescription: pins required_tags persistence\\nversion: 0.1.0\\ntags: [gpu, us-east]\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: tag-persist-pin\\nx-agentsfleet:\\n  triggers:\\n    - type: cron\\n      schedule: '*/30 * * * *'\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    const tags = try requiredTagsCsv(conn, alloc, "tag-persist-pin");
    defer alloc.free(tags);
    try std.testing.expectEqualStrings("gpu,us-east", tags);
}

/// Counts fleets in the test workspace with a given name. COUNT(*) is a
/// single-row result — drained by the lone `next()` before deinit.
fn fleetCountByName(conn: *pg.Conn, name: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*) FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2",
        .{ TEST_WORKSPACE_ID, name },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.CountRowMissing;
    return try row.get(i64, 0);
}

// Spec §5 multi-instance: an optional `name` overrides the SKILL.md-derived
// name, so one source can back two fleets in a single workspace without the
// per-(workspace,name) unique constraint firing UZ-AGT-006. Pins both the
// override-persists path and the no-collision-on-distinct-names path through
// the real router.
test "integration: fleet create name override enables same-source multi-instance" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    // SKILL.md + TRIGGER.md both name themselves "pr-reviewer"; each install
    // overrides the persisted name, so the two coexist in one workspace.
    const skill = "---\\nname: pr-reviewer\\ndescription: reviews prs\\nversion: 0.1.0\\n---\\nBody.\\n";
    const trigger = "---\\nname: pr-reviewer\\nx-agentsfleet:\\n  triggers:\\n    - type: api\\n  tools: []\\n  budget:\\n    daily_dollars: 1.0\\n---\\n";
    const body_a = "{\"source_markdown\":\"" ++ skill ++ "\",\"trigger_markdown\":\"" ++ trigger ++ "\",\"name\":\"pr-reviewer-acme\"}";
    const body_b = "{\"source_markdown\":\"" ++ skill ++ "\",\"trigger_markdown\":\"" ++ trigger ++ "\",\"name\":\"pr-reviewer-blog\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);

    const r_a = try (try (try h.post(url).bearer(TOKEN_USER)).json(body_a)).send();
    defer r_a.deinit();
    try r_a.expectStatus(.created);

    const r_b = try (try (try h.post(url).bearer(TOKEN_USER)).json(body_b)).send();
    defer r_b.deinit();
    try r_b.expectStatus(.created);

    // Both persisted under the override names; the bundle's own "pr-reviewer"
    // name was never written.
    try std.testing.expectEqual(@as(i64, 1), try fleetCountByName(conn, "pr-reviewer-acme"));
    try std.testing.expectEqual(@as(i64, 1), try fleetCountByName(conn, "pr-reviewer-blog"));
    try std.testing.expectEqual(@as(i64, 0), try fleetCountByName(conn, "pr-reviewer"));
}

// Spec §5: an invalid override name (uppercase / spaces / >64 chars) is rejected
// at the write boundary with 400 — the handler wires validateSkillName.
test "integration: fleet create rejects an invalid name override" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    const body =
        "{\"source_markdown\":\"---\\nname: pr-reviewer\\ndescription: d\\nversion: 0.1.0\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: pr-reviewer\\nx-agentsfleet:\\n  triggers:\\n    - type: api\\n  tools: []\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"," ++
        "\"name\":\"Not A Slug\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
}

// Spec Dimension 1.2: malformed required_tags → UZ-REQ-001. The validator's
// bounds are unit-tested exhaustively (config_types); THIS proves the create
// handler WIRES the rejection — an over-long tag (>64 chars) fails the request
// rather than silently storing a never-matching label.
test "integration: fleet create rejects an over-long required tag with UZ-REQ-001" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    const long_tag = "a" ** 65; // one over the 64-char per-tag bound
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"source_markdown\":\"---\\nname: bad-tag-pin\\ndescription: d\\nversion: 0.1.0\\ntags: [{s}]\\n---\\nBody.\\n\"," ++
            "\"trigger_markdown\":\"---\\nname: bad-tag-pin\\nx-agentsfleet:\\n  triggers:\\n    - type: cron\\n      schedule: '*/30 * * * *'\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}}",
        .{long_tag},
    );
    defer alloc.free(body);

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-REQ-001");
}

// Patch re-derivation: editing the SKILL.md via PATCH source_markdown must
// re-stamp required_tags from the reparsed frontmatter (not leave it stale).
// Create untagged → PATCH in `tags: [gpu]` → the column reflects the new set.
test "integration: fleet patch re-derives required_tags from reparsed source_markdown" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedWorkspace(conn, now_ms);

    const create_body =
        "{\"source_markdown\":\"---\\nname: patch-tag-pin\\ndescription: starts untagged\\nversion: 0.1.0\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: patch-tag-pin\\nx-agentsfleet:\\n  triggers:\\n    - type: cron\\n      schedule: '*/30 * * * *'\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}";
    const create_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer alloc.free(create_url);
    const cr = try (try (try h.post(create_url).bearer(TOKEN_USER)).json(create_body)).send();
    defer cr.deinit();
    try cr.expectStatus(.created);

    // Untagged on create.
    const before = try requiredTagsCsv(conn, alloc, "patch-tag-pin");
    defer alloc.free(before);
    try std.testing.expectEqualStrings("", before);

    // Resolve the id, then PATCH a source_markdown that adds `tags: [gpu]`.
    const zid = blk: {
        var q = PgQuery.from(try conn.query(
            "SELECT id::text FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2",
            .{ TEST_WORKSPACE_ID, "patch-tag-pin" },
        ));
        defer q.deinit();
        const row = try q.next() orelse return error.FleetRowMissing;
        break :blk try alloc.dupe(u8, try row.get([]const u8, 0));
    };
    defer alloc.free(zid);

    const patch_body =
        "{\"source_markdown\":\"---\\nname: patch-tag-pin\\ndescription: now tagged\\nversion: 0.1.0\\ntags: [gpu]\\n---\\nBody.\\n\"}";
    const patch_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, zid });
    defer alloc.free(patch_url);
    const pr = try (try (try h.request(.PATCH, patch_url).bearer(TOKEN_USER)).json(patch_body)).send();
    defer pr.deinit();
    try pr.expectStatus(.ok);

    const after = try requiredTagsCsv(conn, alloc, "patch-tag-pin");
    defer alloc.free(after);
    try std.testing.expectEqualStrings("gpu", after);
}
