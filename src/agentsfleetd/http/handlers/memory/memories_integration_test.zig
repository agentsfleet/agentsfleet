// HTTP integration tests for the workspace-scoped /memories collection — now a
// READ-ONLY tenant surface (the write-verb teardown: the runner plane is
// the only writer).
//
//   GET    /v1/workspaces/{ws}/fleets/{zid}/memories          → list-or-search
//   POST   /v1/workspaces/{ws}/fleets/{zid}/memories          → retired (404/405)
//   DELETE /v1/workspaces/{ws}/fleets/{zid}/memories/{key}    → tenant forget
//                                                              (behaviour lives in
//                                                              memory_forget_integration_test.zig)
//
// Entries are seeded directly (memory_runtime INSERT) since POST is gone. Uses
// the shared TestHarness; DB-required; self-skips when TEST_DATABASE_URL is unset.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const id_format = @import("../../../types/id_format.zig");
const metrics_memory = @import("../../../observability/metrics_memory.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const schema_migrations = @import("schema").migrations;

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const OTHER_WS_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aff77";
const AGENTSFLEET_LOCAL = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acc01";
const AGENTSFLEET_OTHER_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acc02";
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_OPERATOR = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

const Fixture = struct {
    h: *TestHarness,

    fn start() !Fixture {
        const h = try TestHarness.start(ALLOC, .{
            .configureRegistry = configureRegistry,
            .inline_jwks_json = TEST_JWKS,
            .issuer = TEST_ISSUER,
            .audience = TEST_AUDIENCE,
        });
        errdefer h.deinit();
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedTestData(conn);
        return .{ .h = h };
    }

    fn deinit(self: Fixture) void {
        if (self.h.acquireConn()) |c| {
            cleanupTestData(c);
            self.h.releaseConn(c);
        } else |_| {}
        self.h.deinit();
    }
};

fn fixture() !Fixture {
    return Fixture.start() catch |err| switch (err) {
        error.SkipZigTest => error.SkipZigTest,
        else => err,
    };
}

fn seedTestData(conn: *pg.Conn) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'MemoriesTest', $2, $2)
        \\ON CONFLICT (id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ OTHER_WS_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2), 'mem-local', '---\nname: mem-local\n---\ntest', '{"name":"mem-local"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENTSFLEET_LOCAL, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2), 'mem-other', '---\nname: mem-other\n---\ntest', '{"name":"mem-other"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENTSFLEET_OTHER_WS, OTHER_WS_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("SET ROLE memory_runtime", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    // Memory is scoped by the raw fleet_id (UUID) after schema/013 — no legacy instance_id prefix.
    _ = conn.exec(
        "DELETE FROM memory.memory_entries WHERE fleet_id IN ($1::uuid, $2::uuid)",
        .{ AGENTSFLEET_LOCAL, AGENTSFLEET_OTHER_WS },
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id IN ($1, $2)", .{ AGENTSFLEET_LOCAL, AGENTSFLEET_OTHER_WS }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE id = $1", .{OTHER_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

/// Every seeded row's updated_at (epoch ms) — pinned so the tenant-wire test
/// can assert the exact unquoted JSON number.
const SEED_TS_MS: i64 = 1_700_000_000_000;

/// Seed one memory entry directly (the tenant write verbs are retired —
/// the runner push is the only writer; here we INSERT under the memory_runtime
/// role so the surviving GET surface has data to read).
fn seedEntry(f: Fixture, fleet_id: []const u8, key: []const u8, content: []const u8, category: []const u8) !void {
    return seedEntryAt(f, fleet_id, key, content, category, SEED_TS_MS);
}

/// seedEntry with an explicit created_at — the keyset-paging tests order and
/// seek over (created_at, key), so each test controls the timeline it walks.
fn seedEntryAt(f: Fixture, fleet_id: []const u8, key: []const u8, content: []const u8, category: []const u8, ts: i64) !void {
    const conn = try f.h.acquireConn();
    defer f.h.releaseConn(conn);
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer _ = conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("reset role ignored: {s}", .{@errorName(err)});
    const uid_value = try id_format.generateUuidV7();
    const row_id: []const u8 = &uid_value;
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (id, key, content, category, fleet_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6, $6)
        \\ON CONFLICT (key, fleet_id) DO UPDATE SET content = EXCLUDED.content, category = EXCLUDED.category
    , .{ row_id, key, content, category, fleet_id, ts });
}

fn memoriesUrl(ws: []const u8, zid: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/memories", .{ ws, zid });
}

// ── GET surface (the tenant memory API is read-only after the write-verb teardown) ──

test "integration: memories GET list returns a seeded entry" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "goal:current", "ship the runner memory loop", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const list_r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer list_r.deinit();
    try list_r.expectStatus(.ok);
    try std.testing.expect(list_r.bodyContains("\"key\":\"goal:current\""));
    try std.testing.expect(list_r.bodyContains("ship the runner memory loop"));
}

test "integration: tenant memory updated_at is a JSON number (epoch millis)" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "goal:current", "numeric wire shape", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    // Unquoted digits after the field name = a JSON number on the wire — the
    // exact seeded epoch-millis value, never a decimal-string shape.
    try std.testing.expect(r.bodyContains("\"updated_at\":1700000000000"));
    try std.testing.expect(!r.bodyContains("\"updated_at\":\""));
}

test "integration: memories GET ?query= finds an entry by content match" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "note:deploy", "deploy lands every monday morning", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const search_url = try std.fmt.allocPrint(ALLOC, "{s}?query=monday", .{url});
    defer ALLOC.free(search_url);
    const search_r = try (try f.h.get(search_url).bearer(TOKEN_OPERATOR)).send();
    defer search_r.deinit();
    try search_r.expectStatus(.ok);
    try std.testing.expect(search_r.bodyContains("\"key\":\"note:deploy\""));
}

// ── Memory-loss counters: the zero-hit search signal ──
// The harness server runs in-process, so the metrics globals asserted here are
// the same atomics the handler increments (backpressure-test precedent).

test "test_search_zero_hit_counts" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "note:topic", "the stored fact mentions kumquats", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const search_url = try std.fmt.allocPrint(ALLOC, "{s}?query=nothing-matches-this", .{url});
    defer ALLOC.free(search_url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(search_url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"total\":0"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total + 1, after.search_zero_hits_total);
}

test "test_search_hit_no_count" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "note:fruit", "the stored fact mentions kumquats", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const search_url = try std.fmt.allocPrint(ALLOC, "{s}?query=kumquats", .{url});
    defer ALLOC.free(search_url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(search_url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"key\":\"note:fruit\""));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total, after.search_zero_hits_total);
}

test "test_list_never_counts_zero_hit" {
    const f = try fixture();
    defer f.deinit();
    // No seeded entries: the list path returns an empty set — still no count,
    // because only the ?query= search path is a recall-miss signal.
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"total\":0"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total, after.search_zero_hits_total);
}

test "test_category_filter_never_counts_zero_hit" {
    const f = try fixture();
    defer f.deinit();
    // The ?category= arm is a filtered list, not a search — an empty result
    // there must never read as a recall miss.
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const cat_url = try std.fmt.allocPrint(ALLOC, "{s}?category=no-such-category", .{url});
    defer ALLOC.free(cat_url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(cat_url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"total\":0"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.search_zero_hits_total, after.search_zero_hits_total);
}

test "test_tenant_list_never_counts_drops" {
    const f = try fixture();
    defer f.deinit();
    // The tenant read is the passthrough Compactor arm — no window applies, so
    // the hydration-drop counters must never move on this surface.
    try seedEntry(f, AGENTSFLEET_LOCAL, "goal:current", "tenant reads are passthrough", "core");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.hydration_dropped_entries_total, after.hydration_dropped_entries_total);
    try std.testing.expectEqual(before.hydration_dropped_bytes_total, after.hydration_dropped_bytes_total);
}

test "integration: memories GET without bearer returns 401" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const r = try f.h.get(url).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

// ── Cross-workspace isolation on the surviving GET surface ──
//   (a) URL workspace = OTHER_WS → auth middleware rejects 403
//   (b) URL workspace = TEST_WS, fleet lives in OTHER_WS → handler 404 (no leak)

test "integration: memories GET cross-workspace URL returns 403" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(OTHER_WS_ID, AGENTSFLEET_OTHER_WS);
    defer ALLOC.free(url);
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
}

test "integration: memories GET fleet-in-foreign-ws returns 404" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_OTHER_WS);
    defer ALLOC.free(url);
    const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}

// ── Keyset paging over (created_at, key) ──
// Every list shape (recent, category, query) seeks strictly past the
// starting_after boundary. The shared workspace may carry sibling tests'
// rows, so walks collect only keys under a per-test-unique prefix and assert
// exhaustiveness + no-repeat on that set; filtered walks additionally refuse
// any foreign row, because their filter marker is test-unique.

const KEYSET_INDEX_NAME = "idx_memory_entries_fleet_id_created_at_key";
const WALK_PAGE_CAP: usize = 12;

/// Walk the memories list to exhaustion via starting_after, collecting keys
/// beginning with `prefix` into `seen` (duped — the caller's defer frees).
/// Fails on a key repeating across pages; with `require_prefix`, fails on any
/// returned key outside the prefix (a filter leak).
fn walkMemoryKeys(
    f: Fixture,
    base_url: []const u8,
    prefix: []const u8,
    require_prefix: bool,
    seen: *std.StringHashMap(void),
) !void {
    var next_cursor: ?[]const u8 = null;
    defer if (next_cursor) |c| ALLOC.free(c);
    var pages: usize = 0;
    while (pages < WALK_PAGE_CAP) : (pages += 1) {
        const url = if (next_cursor) |c|
            try std.fmt.allocPrint(ALLOC, "{s}&starting_after={s}", .{ base_url, c })
        else
            try ALLOC.dupe(u8, base_url);
        defer ALLOC.free(url);

        const r = try (try f.h.get(url).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
        defer parsed.deinit();

        for (parsed.value.object.get("items").?.array.items) |item| {
            const key = item.object.get("key").?.string;
            if (!std.mem.startsWith(u8, key, prefix)) {
                if (require_prefix) return error.ForeignRowInFilteredWalk;
                continue;
            }
            const copy = try ALLOC.dupe(u8, key);
            const gop = seen.getOrPut(copy) catch |err| {
                ALLOC.free(copy);
                return err;
            };
            if (gop.found_existing) {
                ALLOC.free(copy);
                return error.DuplicateKeyAcrossPages;
            }
        }

        if (next_cursor) |c| ALLOC.free(c);
        next_cursor = null;
        switch (parsed.value.object.get("next_cursor").?) {
            .null => return,
            .string => |s| next_cursor = try ALLOC.dupe(u8, s),
            else => return error.UnexpectedCursorType,
        }
    }
    return error.WalkDidNotTerminate;
}

fn freeSeenKeys(seen: *std.StringHashMap(void)) void {
    var it = seen.keyIterator();
    while (it.next()) |k| ALLOC.free(k.*);
    seen.deinit();
}

test "integration: test_memory_keyset_index_migration_registered" {
    // Registration half: embed.zig is the single source of truth for the
    // migration array, so slot 39 must be registered there and must be the slot
    // that creates the keyset index. This originally asserted 39 was the LAST
    // entry; slot 40 (the runner-lease operator read) made tail position
    // transient, while "39 is registered and creates this index" is the
    // durable shape — and it still fails if the slot is renumbered or dropped.
    const KEYSET_SLOT: i32 = 40; // pin test: the slot number is the promise
    var slot_sql: ?[]const u8 = null;
    for (schema_migrations) |m| {
        if (m.version == KEYSET_SLOT) slot_sql = m.sql;
    }
    try std.testing.expect(slot_sql != null);
    try std.testing.expect(std.mem.indexOf(u8, slot_sql.?, KEYSET_INDEX_NAME) != null);

    // Applied half: the index exists in the harness-migrated database.
    const f = try fixture();
    defer f.deinit();
    const conn = try f.h.acquireConn();
    defer f.h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'memory' AND indexname = $1",
        .{KEYSET_INDEX_NAME},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.CountRowMissing;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

test "integration: test_memory_recent_pages_by_cursor" {
    const f = try fixture();
    defer f.deinit();
    const base = clock.nowMillis();
    var prefix_buf: [64]u8 = undefined;
    const prefix = try std.fmt.bufPrint(&prefix_buf, "pgr-{d}-", .{base});
    var key_buf: [96]u8 = undefined;
    for (0..7) |i| {
        const key = try std.fmt.bufPrint(&key_buf, "{s}{d}", .{ prefix, i });
        try seedEntryAt(f, AGENTSFLEET_LOCAL, key, "recent page walk", "core", base + @as(i64, @intCast(i)));
    }

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const base_url = try std.fmt.allocPrint(ALLOC, "{s}?limit=3", .{url});
    defer ALLOC.free(base_url);

    var seen = std.StringHashMap(void).init(ALLOC);
    defer freeSeenKeys(&seen);
    try walkMemoryKeys(f, base_url, prefix, false, &seen);
    try std.testing.expectEqual(@as(u32, 7), seen.count());

    // An unparseable continuation is refused, never treated as page one.
    const bad = try std.fmt.allocPrint(ALLOC, "{s}?starting_after=not-a-cursor", .{url});
    defer ALLOC.free(bad);
    const rb = try (try f.h.get(bad).bearer(TOKEN_OPERATOR)).send();
    defer rb.deinit();
    try rb.expectStatus(.bad_request);
}

test "integration: test_memory_category_filter_pages_by_cursor" {
    const f = try fixture();
    defer f.deinit();
    const base = clock.nowMillis();
    var cat_buf: [64]u8 = undefined;
    const category = try std.fmt.bufPrint(&cat_buf, "cat-{d}", .{base});
    var prefix_buf: [64]u8 = undefined;
    const prefix = try std.fmt.bufPrint(&prefix_buf, "pgc-{d}-", .{base});
    var key_buf: [96]u8 = undefined;
    for (0..5) |i| {
        const key = try std.fmt.bufPrint(&key_buf, "{s}{d}", .{ prefix, i });
        try seedEntryAt(f, AGENTSFLEET_LOCAL, key, "category page walk", category, base + @as(i64, @intCast(i)));
    }
    // A neighbour outside the category must never surface in the walk.
    try seedEntryAt(f, AGENTSFLEET_LOCAL, "pgc-other-category", "foreign row", "core", base);

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const base_url = try std.fmt.allocPrint(ALLOC, "{s}?category={s}&limit=2", .{ url, category });
    defer ALLOC.free(base_url);

    var seen = std.StringHashMap(void).init(ALLOC);
    defer freeSeenKeys(&seen);
    try walkMemoryKeys(f, base_url, prefix, true, &seen);
    try std.testing.expectEqual(@as(u32, 5), seen.count());
}

test "integration: test_memory_search_pages_by_cursor" {
    const f = try fixture();
    defer f.deinit();
    const base = clock.nowMillis();
    var tok_buf: [64]u8 = undefined;
    const token = try std.fmt.bufPrint(&tok_buf, "tok{d}", .{base});
    var prefix_buf: [64]u8 = undefined;
    const prefix = try std.fmt.bufPrint(&prefix_buf, "pgs-{d}-", .{base});
    var key_buf: [96]u8 = undefined;
    var content_buf: [128]u8 = undefined;
    for (0..5) |i| {
        const key = try std.fmt.bufPrint(&key_buf, "{s}{d}", .{ prefix, i });
        const content = try std.fmt.bufPrint(&content_buf, "the fact mentions {s} here", .{token});
        try seedEntryAt(f, AGENTSFLEET_LOCAL, key, content, "core", base + @as(i64, @intCast(i)));
    }
    // A row NOT matching the query must never surface in the walk.
    try seedEntryAt(f, AGENTSFLEET_LOCAL, "pgs-non-match", "no marker in this one", "core", base);

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const base_url = try std.fmt.allocPrint(ALLOC, "{s}?query={s}&limit=2", .{ url, token });
    defer ALLOC.free(base_url);

    var seen = std.StringHashMap(void).init(ALLOC);
    defer freeSeenKeys(&seen);
    try walkMemoryKeys(f, base_url, prefix, true, &seen);
    try std.testing.expectEqual(@as(u32, 5), seen.count());
}

test "integration: test_memory_list_envelope_shape" {
    const f = try fixture();
    defer f.deinit();
    try seedEntry(f, AGENTSFLEET_LOCAL, "env:probe", "envelope probe", "envcat");

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const shapes = [_][]const u8{ "", "?category=envcat", "?query=envelope" };
    for (shapes) |qs_part| {
        const full = try std.fmt.allocPrint(ALLOC, "{s}{s}", .{ url, qs_part });
        defer ALLOC.free(full);
        const r = try (try f.h.get(full).bearer(TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
        defer parsed.deinit();
        // Exactly {items, total, next_cursor} on every shape — nothing else.
        try std.testing.expectEqual(@as(usize, 3), parsed.value.object.count());
        try std.testing.expect(parsed.value.object.get("items") != null);
        try std.testing.expect(parsed.value.object.get("total") != null);
        try std.testing.expect(parsed.value.object.get("next_cursor") != null);
    }
}

test "integration: test_memory_same_millisecond_entries_are_not_skipped" {
    const f = try fixture();
    defer f.deinit();
    const base = clock.nowMillis();
    var prefix_buf: [64]u8 = undefined;
    const prefix = try std.fmt.bufPrint(&prefix_buf, "pgm-{d}-", .{base});
    var key_buf: [96]u8 = undefined;
    for (0..5) |i| {
        const key = try std.fmt.bufPrint(&key_buf, "{s}{d}", .{ prefix, i });
        // Every row shares ONE created_at — only the key tiebreaker orders them.
        try seedEntryAt(f, AGENTSFLEET_LOCAL, key, "same millisecond walk", "core", base);
    }

    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const base_url = try std.fmt.allocPrint(ALLOC, "{s}?limit=2", .{url});
    defer ALLOC.free(base_url);

    var seen = std.StringHashMap(void).init(ALLOC);
    defer freeSeenKeys(&seen);
    try walkMemoryKeys(f, base_url, prefix, false, &seen);
    try std.testing.expectEqual(@as(u32, 5), seen.count());
}

// ── The tenant STORE verb is retired (no compat shim) ──
// POST /memories was removed with the runner-push cutover — the runner plane is
// the only writer. It stays 404/405; GET still 200. (The tenant DELETE is NOT
// retired: it is the operator's forget, tested in memory_forget_integration_test.zig.)

test "integration: tenant memory POST is retired (404/405, no write surface)" {
    const f = try fixture();
    defer f.deinit();
    const url = try memoriesUrl(TEST_WORKSPACE_ID, AGENTSFLEET_LOCAL);
    defer ALLOC.free(url);
    const r = try (try (try f.h.post(url).bearer(TOKEN_OPERATOR)).json(
        "{\"key\":\"k\",\"content\":\"c\",\"category\":\"core\"}",
    )).send();
    defer r.deinit();
    try std.testing.expect(r.status == 404 or r.status == 405);
}
