// The two operator-plane runner reads over the live HTTP surface: the
// single-runner detail (counters from durable state, liveness derived) and the
// keyset lease history joined to its Fleet event. Seeds go straight into the
// real schema; the assertions read the wire.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const scope_fixtures = @import("./test_scope_tokens.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const base = @import("../db/test_fixtures.zig");
const protocol = @import("contract").protocol;
const event_rows = @import("../fleet/event_rows.zig");

const ALLOC = std.testing.allocator;

const PLATFORM_ADMIN_TOKEN = scope_fixtures.PLATFORM_ADMIN;
// VIEWER carries fleet:read + schedule:read only — the persona for proving the
// runner reads refuse a principal without runner:read.
const VIEWER_TOKEN = scope_fixtures.VIEWER;

const WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc01";
const FLEET_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc02";
const FLEET_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc03";
const FLEET_CASCADE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc04";

const R_LEASES = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc10";
const R_COUNTS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc11";
const R_STALE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc12";
const R_EMPTY = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc13";
const R_SAME_MS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc14";
const R_OUTCOME = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc15";
const R_CASCADE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecc16";
const R_UNKNOWN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0eccff";

// A recognisable non-secret sentinel: the read must never emit the column.
const SEEDED_TOKEN_HASH = "cafe0000cafe0000cafe0000cafe0000cafe0000cafe0000cafe0000cafe0000";
const SEEDED_REQUEST_PAYLOAD = "{\"message\":\"never-on-the-wire\"}";
const FAILURE_DETAIL_OOM = "Container exceeded its memory limit and was terminated.";

const LEASE_CREATED_BASE_MS: i64 = 1_750_000_000_000;
/// Spread sequentially-seeded leases one second apart so page order is stable.
const LEASE_CREATED_STEP_MS: i64 = 1000;
const SAME_MS_CREATED_AT: i64 = 1_750_000_100_000;
const LIVE_WINDOW_MS: i64 = 10 * std.time.ms_per_min;
const FAST_ACQUIRE_TIMEOUT_NS: u64 = 200 * std.time.ns_per_ms;
const MAX_HELD_CONNS: usize = 32;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn nowMs() i64 {
    return clock.nowMillis();
}

fn seedRunner(conn: anytype, runner_id: []const u8, host_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', '["gpu","prod"]'::jsonb, $4, $4, $4)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id, SEEDED_TOKEN_HASH, nowMs() });
}

const LeaseSeed = struct {
    lease_id: []const u8,
    runner_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    status: []const u8,
    lease_expires_at: i64,
    created_at: i64,
    fencing_token: i64 = 1,
};

fn seedLease(conn: anytype, seed: LeaseSeed) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, fencing_token, lease_expires_at, status,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'system',
        \\        'chat', $7, 0, 'metered', 'anthropic', 'claude', 18204, 4096, 2881,
        \\        0, $8, $9, $10, $11, $11)
        \\ON CONFLICT (id) DO NOTHING
    , .{
        seed.lease_id,         seed.runner_id, seed.fleet_id,          WS,
        base.TEST_TENANT_ID,   seed.event_id,  SEEDED_REQUEST_PAYLOAD, seed.fencing_token,
        seed.lease_expires_at, seed.status,    seed.created_at,
    });
}

const EventSeed = struct {
    fleet_id: []const u8,
    event_id: []const u8,
    status: []const u8,
    failure_label: ?[]const u8 = null,
    failure_detail: ?[]const u8 = null,
    wall_ms: ?i64 = null,
};

fn seedFleetEvent(conn: anytype, seed: EventSeed) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, wall_ms, failure_label, failure_detail, created_at, updated_at)
        \\VALUES (overlay(md5($1 || $2)::uuid::text placing '7' from 15 for 1)::uuid,
        \\        $1::uuid, $2, $3::uuid, 'system', 'chat', $4, '{}'::jsonb, $5, $6, $7, 0, 0)
        \\ON CONFLICT (fleet_id, event_id) DO NOTHING
    , .{ seed.fleet_id, seed.event_id, WS, seed.status, seed.wall_ms, seed.failure_label, seed.failure_detail });
}

fn seedWorkspaceAndFleets(conn: anytype) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WS);
    try base.seedFleet(conn, FLEET_A, WS, "runner-read-fleet-a", "{}", "");
    try base.seedFleet(conn, FLEET_B, WS, "runner-read-fleet-b", "{}", "");
}

fn cleanup(conn: anytype) void {
    const runner_ids = [_][]const u8{ R_LEASES, R_COUNTS, R_STALE, R_EMPTY, R_SAME_MS, R_OUTCOME, R_CASCADE };
    for (runner_ids) |rid| {
        _ = conn.exec("DELETE FROM fleet.runner_leases WHERE runner_id = $1::uuid", .{rid}) catch |err|
            std.log.warn("lease cleanup ignored: {s}", .{@errorName(err)});
        _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{rid}) catch |err|
            std.log.warn("runner cleanup ignored: {s}", .{@errorName(err)});
    }
    base.teardownFleets(conn, WS);
    base.teardownWorkspace(conn, WS);
    base.teardownTenant(conn);
}

fn runnerPath(runner_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ protocol.PATH_FLEET_RUNNERS, runner_id });
}

fn leasesPath(runner_id: []const u8, query: ?[]const u8) ![]const u8 {
    if (query) |qs| return std.fmt.allocPrint(ALLOC, "{s}/{s}/leases?{s}", .{ protocol.PATH_FLEET_RUNNERS, runner_id, qs });
    return std.fmt.allocPrint(ALLOC, "{s}/{s}/leases", .{ protocol.PATH_FLEET_RUNNERS, runner_id });
}

fn getBody(h: *TestHarness, path: []const u8, token: []const u8) !harness_mod.Response {
    return (try h.get(path).bearer(token)).send();
}

/// Fetch + parse a lease page; caller deinits the returned parse handle.
fn fetchLeases(h: *TestHarness, runner_id: []const u8, query: ?[]const u8) !std.json.Parsed(std.json.Value) {
    const path = try leasesPath(runner_id, query);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
}

test "test_runner_get_omits_token_hash" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_EMPTY, "runner-read-empty");

    const path = try runnerPath(R_EMPTY);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(!resp.bodyContains("token_hash"));
    try std.testing.expect(!resp.bodyContains(SEEDED_TOKEN_HASH));
    // The record itself is present.
    try std.testing.expect(resp.bodyContains("\"host_id\":\"runner-read-empty\""));
    try std.testing.expect(resp.bodyContains("\"admin_state\":\"active\""));
}

test "test_runner_get_unknown_id_is_not_found" {
    const h = try startHarness();
    defer h.deinit();

    const path = try runnerPath(R_UNKNOWN);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.not_found);
    try std.testing.expect(resp.bodyContains("UZ-RUN-014"));
}

test "test_runner_get_requires_runner_read_scope" {
    const h = try startHarness();
    defer h.deinit();

    const path = try runnerPath(R_UNKNOWN);
    defer ALLOC.free(path);
    const detail = try getBody(h, path, VIEWER_TOKEN);
    defer detail.deinit();
    try detail.expectStatus(.forbidden);

    const lease_path = try leasesPath(R_UNKNOWN, null);
    defer ALLOC.free(lease_path);
    const leases = try getBody(h, lease_path, VIEWER_TOKEN);
    defer leases.deinit();
    try leases.expectStatus(.forbidden);
}

test "test_runner_get_counts_distinct_fleets_across_live_leases" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_COUNTS, "runner-read-counts");

    const live_until = nowMs() + LIVE_WINDOW_MS;
    // Three live leases across two fleets.
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd01", .runner_id = R_COUNTS, .fleet_id = FLEET_A, .event_id = "evt-counts-1", .status = protocol.RUNNER_LEASE_STATUS_ACTIVE, .lease_expires_at = live_until, .created_at = LEASE_CREATED_BASE_MS + 1 });
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd02", .runner_id = R_COUNTS, .fleet_id = FLEET_A, .event_id = "evt-counts-2", .status = protocol.RUNNER_LEASE_STATUS_ACTIVE, .lease_expires_at = live_until, .created_at = LEASE_CREATED_BASE_MS + 2 });
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd03", .runner_id = R_COUNTS, .fleet_id = FLEET_B, .event_id = "evt-counts-3", .status = protocol.RUNNER_LEASE_STATUS_ACTIVE, .lease_expires_at = live_until, .created_at = LEASE_CREATED_BASE_MS + 3 });

    const path = try runnerPath(R_COUNTS);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"active_lease_count\":3"));
    try std.testing.expect(resp.bodyContains("\"active_fleet_count\":2"));
    try std.testing.expect(resp.bodyContains("\"liveness\":\"busy\""));
}

test "test_runner_get_lifetime_counters_from_durable_state" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_COUNTS, "runner-read-counts");

    const settled_at = nowMs() - LIVE_WINDOW_MS;
    // 4 reported leases whose events settled processed.
    const processed_ids = [_][]const u8{
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd12",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd13",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd14",
    };
    for (processed_ids, 0..) |lease_id, i| {
        const event_id = try std.fmt.allocPrint(ALLOC, "evt-life-ok-{d}", .{i});
        defer ALLOC.free(event_id);
        try seedLease(conn, .{ .lease_id = lease_id, .runner_id = R_COUNTS, .fleet_id = FLEET_A, .event_id = event_id, .status = protocol.RUNNER_LEASE_STATUS_REPORTED, .lease_expires_at = settled_at, .created_at = LEASE_CREATED_BASE_MS + 10 + @as(i64, @intCast(i)) });
        try seedFleetEvent(conn, .{ .fleet_id = FLEET_A, .event_id = event_id, .status = event_rows.STATUS_PROCESSED });
    }
    // 1 reported lease whose event settled fleet_error.
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd15", .runner_id = R_COUNTS, .fleet_id = FLEET_A, .event_id = "evt-life-bad", .status = protocol.RUNNER_LEASE_STATUS_REPORTED, .lease_expires_at = settled_at, .created_at = LEASE_CREATED_BASE_MS + 20 });
    try seedFleetEvent(conn, .{ .fleet_id = FLEET_A, .event_id = "evt-life-bad", .status = event_rows.STATUS_FLEET_ERROR, .failure_label = "oom_kill", .failure_detail = FAILURE_DETAIL_OOM });
    // 2 expired leases.
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd16", .runner_id = R_COUNTS, .fleet_id = FLEET_B, .event_id = "evt-life-exp-1", .status = protocol.RUNNER_LEASE_STATUS_EXPIRED, .lease_expires_at = settled_at, .created_at = LEASE_CREATED_BASE_MS + 21 });
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd17", .runner_id = R_COUNTS, .fleet_id = FLEET_B, .event_id = "evt-life-exp-2", .status = protocol.RUNNER_LEASE_STATUS_EXPIRED, .lease_expires_at = settled_at, .created_at = LEASE_CREATED_BASE_MS + 22 });

    const path = try runnerPath(R_COUNTS);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"leases_acquired\":7"));
    try std.testing.expect(resp.bodyContains("\"leases_succeeded\":4"));
    try std.testing.expect(resp.bodyContains("\"leases_failed\":1"));
    try std.testing.expect(resp.bodyContains("\"leases_expired\":2"));
}

test "test_runner_get_stale_active_lease_is_not_live" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_STALE, "runner-read-stale");

    // Deadline passed, row still active: neither live nor expired.
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ecd21", .runner_id = R_STALE, .fleet_id = FLEET_A, .event_id = "evt-stale-1", .status = protocol.RUNNER_LEASE_STATUS_ACTIVE, .lease_expires_at = nowMs() - LIVE_WINDOW_MS, .created_at = LEASE_CREATED_BASE_MS + 30 });

    const path = try runnerPath(R_STALE);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"active_lease_count\":0"));
    try std.testing.expect(resp.bodyContains("\"leases_expired\":0"));
    try std.testing.expect(resp.bodyContains("\"leases_acquired\":1"));
}

test "test_runner_leases_envelope_is_items_total_next_cursor" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_EMPTY, "runner-read-empty");

    const parsed = try fetchLeases(h, R_EMPTY, null);
    defer parsed.deinit();
    const obj = parsed.value.object;
    try std.testing.expectEqual(@as(usize, 3), obj.count());
    try std.testing.expect(obj.contains("items"));
    try std.testing.expect(obj.contains("total"));
    try std.testing.expect(obj.contains("next_cursor"));
}

test "test_runner_leases_empty_returns_empty_envelope" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_EMPTY, "runner-read-empty");

    const path = try leasesPath(R_EMPTY, null);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"items\":[]"));
    try std.testing.expect(resp.bodyContains("\"total\":0"));
    try std.testing.expect(resp.bodyContains("\"next_cursor\":null"));
}

/// Seed `count` leases on `runner_id` with strictly increasing `created_at`,
/// lease ids drawn from `id_pool`.
fn seedSequentialLeases(conn: anytype, runner_id: []const u8, id_pool: []const []const u8, created_step: i64) !void {
    for (id_pool, 0..) |lease_id, i| {
        const event_id = try std.fmt.allocPrint(ALLOC, "evt-seq-{s}-{d}", .{ runner_id[32..], i });
        defer ALLOC.free(event_id);
        try seedLease(conn, .{
            .lease_id = lease_id,
            .runner_id = runner_id,
            .fleet_id = FLEET_A,
            .event_id = event_id,
            .status = protocol.RUNNER_LEASE_STATUS_EXPIRED,
            .lease_expires_at = LEASE_CREATED_BASE_MS,
            .created_at = if (created_step == 0) SAME_MS_CREATED_AT else LEASE_CREATED_BASE_MS + @as(i64, @intCast(i)) * created_step,
        });
    }
}

/// Walk the lease list from the first page to exhaustion, returning every item
/// id in arrival order. Asserts each non-final page is exactly `limit` long.
fn walkLeases(h: *TestHarness, runner_id: []const u8, limit: usize) !std.ArrayList([]const u8) {
    var ids: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (ids.items) |id| ALLOC.free(id);
        ids.deinit(ALLOC);
    }
    var cursor: ?[]const u8 = null;
    defer if (cursor) |c| ALLOC.free(c);
    while (true) {
        const query = if (cursor) |c|
            try std.fmt.allocPrint(ALLOC, "limit={d}&starting_after={s}", .{ limit, c })
        else
            try std.fmt.allocPrint(ALLOC, "limit={d}", .{limit});
        defer ALLOC.free(query);

        const parsed = try fetchLeases(h, runner_id, query);
        defer parsed.deinit();
        const obj = parsed.value.object;
        const items = obj.get("items").?.array;
        for (items.items) |item| {
            try ids.append(ALLOC, try ALLOC.dupe(u8, item.object.get("id").?.string));
        }
        const next = obj.get("next_cursor").?;
        if (next == .null) break;
        try std.testing.expectEqual(limit, items.items.len);
        if (cursor) |c| ALLOC.free(c);
        cursor = try ALLOC.dupe(u8, next.string);
    }
    return ids;
}

fn freeIds(ids: *std.ArrayList([]const u8)) void {
    for (ids.items) |id| ALLOC.free(id);
    ids.deinit(ALLOC);
}

fn expectAllUnique(ids: []const []const u8) !void {
    for (ids, 0..) |a, i| {
        for (ids[i + 1 ..]) |b| {
            try std.testing.expect(!std.mem.eql(u8, a, b));
        }
    }
}

test "test_runner_leases_keyset_pages_forward" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_LEASES, "runner-read-leases");

    const pool = [_][]const u8{
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece01",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece02",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece03",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece04",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece05",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece06",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece07",
    };
    try seedSequentialLeases(conn, R_LEASES, &pool, LEASE_CREATED_STEP_MS);

    var ids = try walkLeases(h, R_LEASES, 3);
    defer freeIds(&ids);
    // 7 leases at limit 3 → pages of 3/3/1, nothing repeated, nothing skipped.
    try std.testing.expectEqual(@as(usize, 7), ids.items.len);
    try expectAllUnique(ids.items);
}

test "test_runner_leases_same_millisecond_rows_are_not_skipped" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_SAME_MS, "runner-read-samems");

    const pool = [_][]const u8{
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece12",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece13",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece14",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece15",
    };
    // created_step 0 → every lease shares one created_at millisecond.
    try seedSequentialLeases(conn, R_SAME_MS, &pool, 0);

    var ids = try walkLeases(h, R_SAME_MS, 2);
    defer freeIds(&ids);
    try std.testing.expectEqual(@as(usize, 5), ids.items.len);
    try expectAllUnique(ids.items);
}

test "test_runner_leases_failed_item_carries_failure_fields" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_OUTCOME, "runner-read-outcome");

    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece21", .runner_id = R_OUTCOME, .fleet_id = FLEET_A, .event_id = "evt-fail-1", .status = protocol.RUNNER_LEASE_STATUS_REPORTED, .lease_expires_at = LEASE_CREATED_BASE_MS, .created_at = LEASE_CREATED_BASE_MS });
    try seedFleetEvent(conn, .{ .fleet_id = FLEET_A, .event_id = "evt-fail-1", .status = event_rows.STATUS_FLEET_ERROR, .failure_label = "oom_kill", .failure_detail = FAILURE_DETAIL_OOM, .wall_ms = 242_000 });

    const parsed = try fetchLeases(h, R_OUTCOME, null);
    defer parsed.deinit();
    const items = parsed.value.object.get("items").?.array;
    try std.testing.expectEqual(@as(usize, 1), items.items.len);
    const item = items.items[0].object;
    try std.testing.expectEqualStrings("failed", item.get("outcome").?.string);
    try std.testing.expectEqualStrings("oom_kill", item.get("failure_label").?.string);
    try std.testing.expectEqualStrings(FAILURE_DETAIL_OOM, item.get("failure_detail").?.string);
    try std.testing.expectEqual(@as(i64, 242_000), item.get("wall_ms").?.integer);
}

test "test_runner_leases_never_emits_request_payload" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_OUTCOME, "runner-read-outcome");
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece31", .runner_id = R_OUTCOME, .fleet_id = FLEET_A, .event_id = "evt-payload-1", .status = protocol.RUNNER_LEASE_STATUS_ACTIVE, .lease_expires_at = nowMs() + LIVE_WINDOW_MS, .created_at = LEASE_CREATED_BASE_MS });

    const path = try leasesPath(R_OUTCOME, null);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(!resp.bodyContains("request_json"));
    try std.testing.expect(!resp.bodyContains("never-on-the-wire"));
}

test "test_runner_leases_carries_fleet_link_fields" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_OUTCOME, "runner-read-outcome");
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece41", .runner_id = R_OUTCOME, .fleet_id = FLEET_A, .event_id = "evt-link-1", .status = protocol.RUNNER_LEASE_STATUS_ACTIVE, .lease_expires_at = nowMs() + LIVE_WINDOW_MS, .created_at = LEASE_CREATED_BASE_MS });

    const parsed = try fetchLeases(h, R_OUTCOME, null);
    defer parsed.deinit();
    const item = parsed.value.object.get("items").?.array.items[0].object;
    try std.testing.expectEqualStrings(FLEET_A, item.get("fleet_id").?.string);
    try std.testing.expectEqualStrings(WS, item.get("workspace_id").?.string);
    try std.testing.expectEqualStrings("runner-read-fleet-a", item.get("fleet_name").?.string);
    try std.testing.expectEqualStrings("evt-link-1", item.get("event_id").?.string);
}

test "test_runner_leases_expired_lease_is_not_credited_with_successor_outcome" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_OUTCOME, "runner-read-outcome");

    // This runner's lease expired; the SAME event was re-leased elsewhere and
    // later settled processed. The expired holder still reads expired, and the
    // reclaim (higher fencing token, same event) reads as kind reclaim.
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece51", .runner_id = R_OUTCOME, .fleet_id = FLEET_A, .event_id = "evt-reclaimed-1", .status = protocol.RUNNER_LEASE_STATUS_EXPIRED, .lease_expires_at = LEASE_CREATED_BASE_MS, .created_at = LEASE_CREATED_BASE_MS, .fencing_token = 1 });
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece52", .runner_id = R_OUTCOME, .fleet_id = FLEET_A, .event_id = "evt-reclaimed-1", .status = protocol.RUNNER_LEASE_STATUS_REPORTED, .lease_expires_at = LEASE_CREATED_BASE_MS + LEASE_CREATED_STEP_MS, .created_at = LEASE_CREATED_BASE_MS + LEASE_CREATED_STEP_MS, .fencing_token = 2 });
    try seedFleetEvent(conn, .{ .fleet_id = FLEET_A, .event_id = "evt-reclaimed-1", .status = event_rows.STATUS_PROCESSED });

    const parsed = try fetchLeases(h, R_OUTCOME, null);
    defer parsed.deinit();
    const items = parsed.value.object.get("items").?.array;
    try std.testing.expectEqual(@as(usize, 2), items.items.len);
    for (items.items) |entry| {
        const item = entry.object;
        const id = item.get("id").?.string;
        if (std.mem.eql(u8, id, "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece51")) {
            try std.testing.expectEqualStrings("expired", item.get("outcome").?.string);
            try std.testing.expectEqualStrings("fresh", item.get("kind").?.string);
        } else {
            try std.testing.expectEqualStrings("succeeded", item.get("outcome").?.string);
            try std.testing.expectEqualStrings("reclaim", item.get("kind").?.string);
        }
    }
}

test "test_runner_leases_missing_event_reads_unknown" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_OUTCOME, "runner-read-outcome");
    // Reported lease, no matching Fleet event row at all.
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece61", .runner_id = R_OUTCOME, .fleet_id = FLEET_A, .event_id = "evt-vanished-1", .status = protocol.RUNNER_LEASE_STATUS_REPORTED, .lease_expires_at = LEASE_CREATED_BASE_MS, .created_at = LEASE_CREATED_BASE_MS });

    const parsed = try fetchLeases(h, R_OUTCOME, null);
    defer parsed.deinit();
    const item = parsed.value.object.get("items").?.array.items[0].object;
    try std.testing.expectEqualStrings("unknown", item.get("outcome").?.string);
}

test "test_runner_leases_deleted_fleet_cascades_out" {
    // The schema's ON DELETE CASCADE removes a deleted fleet's leases, so the
    // read simply stops listing them — there is no orphan row to render.
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try base.seedFleet(conn, FLEET_CASCADE, WS, "runner-read-fleet-cascade", "{}", "");
    try seedRunner(conn, R_CASCADE, "runner-read-cascade");
    try seedLease(conn, .{ .lease_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece71", .runner_id = R_CASCADE, .fleet_id = FLEET_CASCADE, .event_id = "evt-cascade-1", .status = protocol.RUNNER_LEASE_STATUS_REPORTED, .lease_expires_at = LEASE_CREATED_BASE_MS, .created_at = LEASE_CREATED_BASE_MS });

    _ = try conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_CASCADE});

    const path = try leasesPath(R_CASCADE, null);
    defer ALLOC.free(path);
    const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(resp.bodyContains("\"items\":[]"));
    try std.testing.expect(resp.bodyContains("\"total\":0"));
}

test "test_runner_leases_rejects_malformed_cursor" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_EMPTY, "runner-read-empty");

    // Not a UUID at all.
    const garbled = try leasesPath(R_EMPTY, "starting_after=not-a-lease-id");
    defer ALLOC.free(garbled);
    const garbled_resp = try getBody(h, garbled, PLATFORM_ADMIN_TOKEN);
    defer garbled_resp.deinit();
    try garbled_resp.expectStatus(.bad_request);
    try std.testing.expect(garbled_resp.bodyContains("UZ-REQ-001"));

    // Well-formed, but not a lease this runner holds.
    const foreign = try leasesPath(R_EMPTY, "starting_after=" ++ R_UNKNOWN);
    defer ALLOC.free(foreign);
    const foreign_resp = try getBody(h, foreign, PLATFORM_ADMIN_TOKEN);
    defer foreign_resp.deinit();
    try foreign_resp.expectStatus(.bad_request);
    try std.testing.expect(foreign_resp.bodyContains("UZ-REQ-001"));
}

test "test_runner_leases_rejects_limit_out_of_range" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_EMPTY, "runner-read-empty");

    const bad_limits = [_][]const u8{ "limit=0", "limit=-1", "limit=abc", "limit=101" };
    for (bad_limits) |qs| {
        const path = try leasesPath(R_EMPTY, qs);
        defer ALLOC.free(path);
        const resp = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
        defer resp.deinit();
        try resp.expectStatus(.bad_request);
        try std.testing.expect(resp.bodyContains("1 and 100"));
    }
}

test "test_runner_leases_repeated_cursor_is_stable" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_LEASES, "runner-read-leases");
    const pool = [_][]const u8{
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece81",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece82",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece83",
    };
    try seedSequentialLeases(conn, R_LEASES, &pool, LEASE_CREATED_STEP_MS);

    const query = "limit=1&starting_after=" ++ "0195b4ba-8d3a-7f13-8abc-2b3e1e0ece83";
    const path = try leasesPath(R_LEASES, query);
    defer ALLOC.free(path);
    const first = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer first.deinit();
    try first.expectStatus(.ok);
    const second = try getBody(h, path, PLATFORM_ADMIN_TOKEN);
    defer second.deinit();
    try second.expectStatus(.ok);
    try std.testing.expectEqualStrings(first.body, second.body);
}

test "test_runner_read_db_unavailable_is_service_error" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanup(conn);
    try seedWorkspaceAndFleets(conn);
    try seedRunner(conn, R_EMPTY, "runner-read-empty");

    // Drain the pool so the handler's acquire fails, with a short poll budget
    // so the refusal is fast. Held connections are returned before teardown.
    h.pool._timeout = FAST_ACQUIRE_TIMEOUT_NS;
    var held: [MAX_HELD_CONNS]?*pg.Conn = @splat(null);
    var held_count: usize = 0;
    while (held_count < MAX_HELD_CONNS) {
        held[held_count] = h.pool.acquire() catch break;
        held_count += 1;
    }
    defer for (held[0..held_count]) |maybe| {
        if (maybe) |c| h.pool.release(c);
    };

    const detail_path = try runnerPath(R_EMPTY);
    defer ALLOC.free(detail_path);
    const detail = try getBody(h, detail_path, PLATFORM_ADMIN_TOKEN);
    defer detail.deinit();
    try detail.expectStatus(.service_unavailable);
    try std.testing.expect(detail.bodyContains("UZ-INTERNAL-001"));

    const lease_path = try leasesPath(R_EMPTY, null);
    defer ALLOC.free(lease_path);
    const leases = try getBody(h, lease_path, PLATFORM_ADMIN_TOKEN);
    defer leases.deinit();
    try leases.expectStatus(.service_unavailable);
    try std.testing.expect(leases.bodyContains("UZ-INTERNAL-001"));
}
