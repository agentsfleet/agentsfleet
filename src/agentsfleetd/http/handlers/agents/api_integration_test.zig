// HTTP integration tests for the agents CRUD API — focused on cursor
// pagination on GET /v1/workspaces/{ws}/agents.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const id_format = @import("../../../types/id_format.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"310oH7ahxoKws6fEKmbOP30dQaQhT21HGRxvibeBuqfywkNxJ0xcfhhao1mwbLH7BUOg2GYXDEA6EvcVlKXqGN_Wa_4Q7UenmZqeXYdB_IhAc-SzyoW9hRi01FskVVI8w_N0Pf5SItu7DIqdxbKP8_eGFyrTL1mN-5klkIDCSnhrDLUEgjVo7iod0vsoqUEH-2m1s-2xDh5aQr5rSF6neCTA1-JvKVkJLD6eOdBnEwYBm6-yZ0CNgMfw1uUyw5cGwdaPsCerHctH0EwcI_qQFUUnFjBeN4FJkP_DDoHWTEV9a-5wzomOcoKlyfZvRgplGYYqTWrIAfcZobyzYiSy1w","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_USER =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJ1c2VyIn19.aSqdpbu-D-1NmzJgcw-7LUJYImlFu-gbrO3fBPlMI6DFvgSGJJg3wAYe5DKJXe5ytCActeAHN8LxGyr1emB4ReHk90B7t_DB301cl5fz6H1EIBnUYkuOYIeCQXvqTmEHduR1KPumEYc6Jfw3kv1tY95k-bugObZ4FihLhWXw4ud8fXRl_CTnD3J3FSx-cn4K8mfy8JjTc1RDmEx5_4-TbBhPyTgj5EAXqB1ddUw7k46UAh_-w2G07SrOxsl1b57Etwp0gvuu4tkpXICYmG423n-RjVvtvuxjSzQyhUZ2Lmfbvi1tLlY7_uzTh_BwwWWYLdJtnmKEblmGReoAu_Qs6A";

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

fn seedAgents(alloc: std.mem.Allocator, conn: *pg.Conn, count: usize, base_ms: i64) ![][]const u8 {
    var ids = try alloc.alloc([]const u8, count);
    errdefer {
        for (ids[0..]) |id| if (id.len > 0) alloc.free(id);
        alloc.free(ids);
    }
    for (0..count) |i| {
        const id = try id_format.generateAgentId(alloc);
        ids[i] = id;
        const name = try std.fmt.allocPrint(alloc, "agent-pg-{d}-{d}", .{ base_ms, i });
        defer alloc.free(name);
        _ = try conn.exec(
            \\INSERT INTO core.agents
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

test "integration: agents list — cursor pagination roundtrip" {
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
    const ids = try seedAgents(alloc, conn, 5, now_ms);
    defer freeIds(alloc, ids);

    // Full cursor round-trip: 5 agents seeded, limit=2 means pages of
    // 2 + 2 + 1. Walk every page, accumulate ids, and assert:
    //   (a) continuation has no overlap with prior pages,
    //   (b) last page carries cursor=null,
    //   (c) union of ids across pages == seeded set (order agnostic).
    var seen_ids = std.StringHashMap(void).init(alloc);
    defer seen_ids.deinit();

    var next_cursor: ?[]const u8 = null;
    var page_count: usize = 0;
    while (page_count < 10) : (page_count += 1) { // hard cap guards runaway loop
        const url = if (next_cursor) |c|
            try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents?limit=2&cursor={s}", .{ TEST_WORKSPACE_ID, c })
        else
            try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents?limit=2", .{TEST_WORKSPACE_ID});
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
    // (c) every seeded agent was returned across the walk.
    try std.testing.expectEqual(@as(usize, 5), seen_ids.count());
    var seen_it = seen_ids.keyIterator();
    while (seen_it.next()) |key_ptr| alloc.free(key_ptr.*);

    // Bad cursor → 400.
    const url_bad = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents?cursor=not-a-cursor", .{TEST_WORKSPACE_ID});
    defer alloc.free(url_bad);
    const r_bad = try (try h.get(url_bad).bearer(TOKEN_USER)).send();
    defer r_bad.deinit();
    try r_bad.expectStatus(.bad_request);

    // No-token → 401.
    const url_anon = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents", .{TEST_WORKSPACE_ID});
    defer alloc.free(url_anon);
    const r_anon = try h.get(url_anon).send();
    defer r_anon.deinit();
    try r_anon.expectStatus(.unauthorized);
}

test "integration: agents list — projects triggers array from config_json" {
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

    // Seed one agent with a `triggers[]` array inside `x-agentsfleet:` — mirrors
    // what config_parser.zig persists for a real install. Bypasses the HTTP
    // create path (Redis required) but exercises the SELECT projection that
    // list.zig adds: `config_json->'x-agentsfleet'->'triggers'`.
    const zid = try id_format.generateAgentId(alloc);
    defer alloc.free(zid);
    const config_json =
        \\{"name":"triggers-projection","x-agentsfleet":{"triggers":[
        \\  {"type":"webhook","source":"github","events":["workflow_run"]},
        \\  {"type":"cron","schedule":"*/30 * * * *"}
        \\]}}
    ;
    _ = try conn.exec(
        \\INSERT INTO core.agents
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'seed', null, $4::jsonb, 'active', $5, $5)
    , .{ zid, TEST_WORKSPACE_ID, "triggers-projection", config_json, now_ms + 9_000 });

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents?limit=20", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);

    const r = try (try h.get(url).bearer(TOKEN_USER)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    // Newest-first ordering puts our agent at index 0 (created_at = now+9000s,
    // greater than any sibling rows from the pagination suite).
    try std.testing.expect(r.bodyContains("\"name\":\"triggers-projection\""));
    try std.testing.expect(r.bodyContains("\"type\":\"webhook\""));
    try std.testing.expect(r.bodyContains("\"source\":\"github\""));
    try std.testing.expect(r.bodyContains("\"workflow_run\""));
    try std.testing.expect(r.bodyContains("\"type\":\"cron\""));
    try std.testing.expect(r.bodyContains("\"schedule\":\"*/30 * * * *\""));
}

// §2 contract: 201 install response carries a `webhook_urls` map keyed by
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

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    try std.testing.expect(r.bodyContains("\"webhook_urls\":{"));
    try std.testing.expect(r.bodyContains("\"github\":\"http://127.0.0.1/v1/webhooks/"));
    try std.testing.expect(r.bodyContains("/github\""));
}

// §Failure Modes row: "Install with no webhook trigger → 201 with
// `webhook_urls: {}`". A cron-only agent short-circuits the install-skill's
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

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    try std.testing.expect(r.bodyContains("\"webhook_urls\":{}"));
}

// Cross-file `name:` invariant: SKILL.md and TRIGGER.md must agree on identity.
// Handler enforcement at create.zig fires before workspace authorization, so a
// USER-role token still surfaces the mismatch error (no escalation needed).
test "integration: agent create rejects SKILL/TRIGGER name mismatch with UZ-AGT-011" {
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

    // SKILL.md says alpha-agent; TRIGGER.md says beta-agent. Both halves
    // parse cleanly in isolation — the rejection only fires at the install
    // handler, which is what this test pins.
    const body =
        "{\"source_markdown\":\"---\\nname: alpha-agent\\ndescription: alpha\\nversion: 0.1.0\\n---\\nBody.\\n\"," ++
        "\"trigger_markdown\":\"---\\nname: beta-agent\\nx-agentsfleet:\\n  triggers:\\n    - type: webhook\\n      source: agentmail\\n  tools:\\n    - agentmail\\n  budget:\\n    daily_dollars: 1.0\\n---\\n\"}";

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-AGT-011");
}

/// Reads the stored required_tags for a agent (by name) as a comma-joined
/// string so the assertion is order-explicit. Row-backed slice is compared
/// in-function before deinit.
fn requiredTagsCsv(conn: *pg.Conn, alloc: std.mem.Allocator, name: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query(
        "SELECT array_to_string(ARRAY(SELECT unnest(required_tags) ORDER BY 1), ',') FROM core.agents WHERE workspace_id = $1::uuid AND name = $2",
        .{ TEST_WORKSPACE_ID, name },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.AgentRowMissing;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

// Spec Dimension 1.1: a agent persists required_tags from create. The placement
// eligibility suite seeds tags via a raw UPDATE; THIS test proves the create
// handler actually writes the SKILL.md frontmatter `tags:` into the column —
// the S3 persistence path, end-to-end through the real router.
test "integration: agent create persists SKILL.md tags into required_tags" {
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

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    const tags = try requiredTagsCsv(conn, alloc, "tag-persist-pin");
    defer alloc.free(tags);
    try std.testing.expectEqualStrings("gpu,us-east", tags);
}

// Spec Dimension 1.2: malformed required_tags → UZ-REQ-001. The validator's
// bounds are unit-tested exhaustively (config_types); THIS proves the create
// handler WIRES the rejection — an over-long tag (>64 chars) fails the request
// rather than silently storing a never-matching label.
test "integration: agent create rejects an over-long required tag with UZ-REQ-001" {
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

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents", .{TEST_WORKSPACE_ID});
    defer alloc.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.bad_request);
    try r.expectErrorCode("UZ-REQ-001");
}

// Patch re-derivation: editing the SKILL.md via PATCH source_markdown must
// re-stamp required_tags from the reparsed frontmatter (not leave it stale).
// Create untagged → PATCH in `tags: [gpu]` → the column reflects the new set.
test "integration: agent patch re-derives required_tags from reparsed source_markdown" {
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
    const create_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents", .{TEST_WORKSPACE_ID});
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
            "SELECT id::text FROM core.agents WHERE workspace_id = $1::uuid AND name = $2",
            .{ TEST_WORKSPACE_ID, "patch-tag-pin" },
        ));
        defer q.deinit();
        const row = try q.next() orelse return error.AgentRowMissing;
        break :blk try alloc.dupe(u8, try row.get([]const u8, 0));
    };
    defer alloc.free(zid);

    const patch_body =
        "{\"source_markdown\":\"---\\nname: patch-tag-pin\\ndescription: now tagged\\nversion: 0.1.0\\ntags: [gpu]\\n---\\nBody.\\n\"}";
    const patch_url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/agents/{s}", .{ TEST_WORKSPACE_ID, zid });
    defer alloc.free(patch_url);
    const pr = try (try (try h.request(.PATCH, patch_url).bearer(TOKEN_USER)).json(patch_body)).send();
    defer pr.deinit();
    try pr.expectStatus(.ok);

    const after = try requiredTagsCsv(conn, alloc, "patch-tag-pin");
    defer alloc.free(after);
    try std.testing.expectEqualStrings("gpu", after);
}
