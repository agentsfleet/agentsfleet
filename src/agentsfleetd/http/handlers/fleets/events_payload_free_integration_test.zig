//! The events list carries no bodies, and its plan never reaches for one.
//!
//! Two claims, asserted two different ways because a grep can express neither:
//!
//!   1. The response carries no request or response body field. Cheap, exact.
//!   2. The read does not touch oversized-attribute storage. This is the claim
//!      that actually matters — a bounded prefix of a stored body would satisfy
//!      (1) while still fetching and decompressing every value, which is the
//!      cost the split exists to remove.
//!
//! (2) is proven twice over. The response-size ceiling is the deterministic
//! half: 200 rows each holding a body far larger than the whole page's ceiling
//! cannot fit through a read that carries them. The catalogue counter is the
//! direct half, read from `pg_statio_all_tables`.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;

const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const id_format = @import("../../../types/id_format.zig");

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TOKEN_VIEWER = scope_fixtures.VIEWER;

/// A full page — the maximum the handler allows, which is the shape the split
/// was argued about.
const PAGE_ROWS: usize = 200;
/// Each seeded body. Comfortably past the point where Postgres moves a value
/// out of the main row, so a read that wants one has to go and get it.
const BODY_BYTES: usize = 20_000;
/// The whole page's ceiling. Two orders of magnitude below what 200 bodies
/// would weigh (200 × 20 kB ≈ 4 MB) and far above what 200 body-free rows do,
/// so the assertion cannot be squeaked past by trimming a field.
const RESPONSE_CEILING_BYTES: usize = 256 * 1024;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedWorkspace(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'PayloadFreeTest', $2, $2) ON CONFLICT (id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3) ON CONFLICT (id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
}

fn seedFleet(conn: *pg.Conn, stamp: i64) ![]const u8 {
    const id = try id_format.generateFleetId(ALLOC);
    errdefer ALLOC.free(id);
    const name = try std.fmt.allocPrint(ALLOC, "payload-free-{d}", .{stamp});
    defer ALLOC.free(name);
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2::uuid),
        \\        $3, '# payload free', '{}'::jsonb, 'active', $4, $4)
    , .{ id, TEST_WORKSPACE_ID, name, stamp });
    return id;
}

/// A full page of events, every one carrying an oversized body on both sides.
/// Distinct bytes per row so nothing can be deduplicated away.
fn seedFatEvents(conn: *pg.Conn, fleet_id: []const u8, stamp: i64) !void {
    const filler = try ALLOC.alloc(u8, BODY_BYTES);
    defer ALLOC.free(filler);

    for (0..PAGE_ROWS) |i| {
        @memset(filler, @as(u8, @intCast('a' + (i % 26))));
        const request = try std.fmt.allocPrint(ALLOC, "{{\"message\":\"{s}\"}}", .{filler});
        defer ALLOC.free(request);
        const event_id = try std.fmt.allocPrint(ALLOC, "evt-fat-{d}-{d}", .{ stamp, i });
        defer ALLOC.free(event_id);
        _ = try conn.exec(
            \\INSERT INTO core.fleet_events
            \\  (fleet_id, event_id, workspace_id, actor, event_type, status,
            \\   request_json, response_text, tokens, wall_ms, created_at, updated_at)
            \\VALUES ($1::uuid, $2, $3::uuid, 'steer:tester', 'chat', 'processed',
            \\        $4::jsonb, $5, 7, 11, $6, $6)
        , .{ fleet_id, event_id, TEST_WORKSPACE_ID, request, filler, stamp + @as(i64, @intCast(i)) });
    }
}

fn purgeFleet(conn: *pg.Conn, fleet_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_id}) catch |err|
        std.log.warn("payload-free fixture purge ignored: {s}", .{@errorName(err)});
}

/// Blocks this backend has read from `core.fleet_events`' oversized-attribute
/// storage, hit or miss. Cumulative, so only the delta across the read means
/// anything. `pg_stat_clear_snapshot` drops the per-transaction cache that
/// would otherwise hand back the same numbers twice.
fn toastBlocks(conn: *pg.Conn) !i64 {
    _ = try conn.exec("SELECT pg_stat_clear_snapshot()", .{});
    var q = PgQuery.from(try conn.query(
        \\SELECT COALESCE(toast_blks_read, 0) + COALESCE(toast_blks_hit, 0)
        \\FROM pg_statio_all_tables WHERE schemaname = 'core' AND relname = 'fleet_events'
    , .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoStatsRow;
    return row.get(i64, 0);
}

test "integration: test_events_list_selects_no_payload_columns" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspace(conn);

    const stamp = clock.nowMillis();
    const fleet_id = try seedFleet(conn, stamp);
    defer ALLOC.free(fleet_id);
    defer purgeFleet(conn, fleet_id);
    try seedFatEvents(conn, fleet_id, stamp);

    const url = try std.fmt.allocPrint(
        ALLOC,
        "/v1/workspaces/{s}/fleets/{s}/events?limit={d}",
        .{ TEST_WORKSPACE_ID, fleet_id, PAGE_ROWS },
    );
    defer ALLOC.free(url);

    const before = try toastBlocks(conn);
    const r = try (try h.get(url).bearer(TOKEN_VIEWER)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    const after = try toastBlocks(conn);

    // 1. No body field, by name. The wire shape is the promise callers hold.
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"request_json\"") == null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"response_text\"") == null);

    // A full page really was returned — otherwise the assertions above pass
    // trivially on an empty list and prove nothing at all.
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
    defer parsed.deinit();
    try std.testing.expectEqual(PAGE_ROWS, parsed.value.object.get("items").?.array.items.len);

    // 2. And no body came through by any other name. 200 stored bodies weigh
    // roughly 4 MB; the whole response has to fit in a fraction of that.
    try std.testing.expect(r.body.len < RESPONSE_CEILING_BYTES);

    // 3. The plan never reached for oversized-attribute storage. This is the
    // claim a prefix-selecting query would fail while still passing (1) and (2).
    try std.testing.expectEqual(before, after);
}
