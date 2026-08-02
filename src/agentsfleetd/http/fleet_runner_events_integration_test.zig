// Runner event history over the live HTTP surface.

const std = @import("std");
const scope_fixtures = @import("./test_scope_tokens.zig");
const clock = @import("common").clock;
const auth_mw = @import("../auth/middleware/mod.zig");
const api_key = @import("../auth/api_key.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const protocol = @import("contract").protocol;
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const redis_fleet = @import("../queue/redis_fleet.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const REGISTER_HOST = "host-event-register-test";
const REGISTER_BODY =
    \\{"host_id":"host-event-register-test","assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":1},"labels":[]}
;
const BODY_CORDON = "{\"action\":\"cordon\"}";

const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6c01";
const RUNNER_TOKEN_BODY_HEX_CHARS: usize = 64;
const RUNNER_TOKEN = protocol.RUNNER_TOKEN_PREFIX ++ "e" ** RUNNER_TOKEN_BODY_HEX_CHARS;
const LARGE_BALANCE_NANOS: i64 = 1_000_000_000_000;
const REPORT_TOKENS: u64 = 10;
const REPORT_WALL_MS: u64 = 100;
const REPORT_TTFT_MS: u32 = 5;
const SQL_INSTALL_HEARTBEAT_EVENT_REJECTOR =
    \\DROP TRIGGER IF EXISTS reject_runner_event_test ON fleet.runner_events;
    \\DROP FUNCTION IF EXISTS fleet.reject_runner_event_test();
    \\CREATE FUNCTION fleet.reject_runner_event_test()
    \\RETURNS trigger
    \\LANGUAGE plpgsql
    \\AS $$
    \\BEGIN
    \\  RAISE EXCEPTION 'reject runner event test';
    \\END;
    \\$$;
    \\CREATE TRIGGER reject_runner_event_test
    \\BEFORE INSERT ON fleet.runner_events
    \\FOR EACH ROW EXECUTE FUNCTION fleet.reject_runner_event_test();
;
const SQL_DROP_HEARTBEAT_EVENT_REJECTOR =
    \\DROP TRIGGER IF EXISTS reject_runner_event_test ON fleet.runner_events;
    \\DROP FUNCTION IF EXISTS fleet.reject_runner_event_test();
;
const SQL_SELECT_RUNNER_LAST_SEEN =
    \\SELECT last_seen_at FROM fleet.runners WHERE id = $1::uuid
;
const CLEANUP_HEARTBEAT_REJECTOR_IGNORED_FMT = "cleanup heartbeat event rejector ignored: {s}";

const CONFIG_NO_GATES =
    \\{"name":"runner-events-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: runner-events-bot
    \\---
    \\
    \\You are a runner event test fleet.
;

const TEST_JWKS = scope_fixtures.JWKS;
const PLATFORM_ADMIN_TOKEN = scope_fixtures.PLATFORM_ADMIN;

// SAFETY: populated by configureRegistry before runnerBearer reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedRunner(conn: anytype) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'runner-events-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

fn seedFleetWork(conn: anytype) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'runner-events-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
    try seedRunner(conn);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "runner-events-fleet", CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, FLEET_ID, "{}");
}

fn publishFreshEvent(h: *TestHarness) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, FLEET_ID);
    const id = try redis_fleet.xaddFleetEvent(&h.queue, .{
        .event_id = "",
        .fleet_id = FLEET_ID,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:runner-events",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

const LeaseView = struct {
    lease_id: []const u8,
    event_id: []const u8,
    fencing_token: u64,
};

fn parseLease(body: []const u8) !LeaseView {
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease") orelse return error.TestUnexpectedResult;
    if (lease == .null) return error.TestUnexpectedResult;
    const obj = lease.object;
    return .{
        .lease_id = try ALLOC.dupe(u8, obj.get("lease_id").?.string),
        .event_id = try ALLOC.dupe(u8, obj.get("event").?.object.get("event_id").?.string),
        .fencing_token = @intCast(obj.get("fencing_token").?.integer),
    };
}

fn freeLease(v: LeaseView) void {
    ALLOC.free(v.lease_id);
    ALLOC.free(v.event_id);
}

fn leaseOnce(h: *TestHarness) !LeaseView {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return parseLease(resp.body);
}

fn reportLease(h: *TestHarness, lease: LeaseView) !harness_mod.Response {
    const body = try std.fmt.allocPrint(ALLOC,
        \\{{"lease_id":"{s}","event_id":"{s}","fencing_token":{d},"outcome":"processed","response_text":"done","tokens":{d},"telemetry":{{"time_to_first_token_ms":{d},"wall_ms":{d}}},"checkpoint":{{"last_event_id":"{s}","last_response":"done"}}}}
    , .{ lease.lease_id, lease.event_id, lease.fencing_token, REPORT_TOKENS, REPORT_TTFT_MS, REPORT_WALL_MS, lease.event_id });
    defer ALLOC.free(body);
    const req = try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(RUNNER_TOKEN)).json(body);
    return req.send();
}

fn eventsPath(runner_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}/events?limit=10", .{ protocol.PATH_FLEET_RUNNERS, runner_id });
}

fn eventsPathWithQuery(runner_id: []const u8, query: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}/events?{s}", .{ protocol.PATH_FLEET_RUNNERS, runner_id, query });
}

fn patchPath(runner_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ protocol.PATH_FLEET_RUNNERS, runner_id });
}

fn eventCount(conn: anytype, runner_id: []const u8, event_type: protocol.RunnerEventType) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::bigint FROM fleet.runner_events
        \\WHERE runner_id = $1::uuid AND event_type = $2
    , .{ runner_id, @tagName(event_type) }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

fn registeredRunnerId(conn: anytype) ![]const u8 {
    var q = PgQuery.from(try conn.query("SELECT id::text FROM fleet.runners WHERE host_id = $1", .{REGISTER_HOST}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return ALLOC.dupe(u8, try row.get([]const u8, 0));
}

/// Helper-scoped so the PgQuery drains on return — an inline query with a
/// deferred deinit holds the connection busy for the next statement.
fn runnerRowCount(conn: anytype, runner_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::bigint FROM fleet.runners WHERE id = $1::uuid
    , .{runner_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

fn cleanupRegister(conn: anytype) void {
    _ = conn.exec("DELETE FROM fleet.runners WHERE host_id = $1", .{REGISTER_HOST}) catch |err|
        std.log.warn("cleanup registered runner ignored: {s}", .{@errorName(err)});
}

fn cleanupFleetWork(h: *TestHarness, conn: anytype) void {
    // Stream AND readiness mark: `fleet:ready` is one shared key and `peek` is
    // bounded + randomized, so a mark left for a fleet this teardown deletes can
    // crowd a sibling suite's fleet out of the sample.
    redis_fleet.purgeFleetRedisState(&h.queue, FLEET_ID) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID}) catch |err|
        std.log.warn("cleanup runner ignored: {s}", .{@errorName(err)});
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

fn installHeartbeatEventRejector(conn: anytype) !void {
    _ = try conn.exec(SQL_INSTALL_HEARTBEAT_EVENT_REJECTOR, .{});
}

fn dropHeartbeatEventRejector(conn: anytype) void {
    _ = conn.exec(SQL_DROP_HEARTBEAT_EVENT_REJECTOR, .{}) catch |err|
        std.log.warn(CLEANUP_HEARTBEAT_REJECTOR_IGNORED_FMT, .{@errorName(err)});
}

fn runnerLastSeen(conn: anytype, runner_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(SQL_SELECT_RUNNER_LAST_SEEN, .{runner_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

test "integration: state writes append runner events and history route lists them" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupRegister(conn);

    const register = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer register.deinit();
    try register.expectStatus(.created);
    const runner_id = try registeredRunnerId(conn);
    defer ALLOC.free(runner_id);
    try std.testing.expectEqual(@as(i64, 1), try eventCount(conn, runner_id, .runner_registered));

    const p = try patchPath(runner_id);
    defer ALLOC.free(p);
    const cordon = try (try (try h.request(.PATCH, p).bearer(PLATFORM_ADMIN_TOKEN)).json(BODY_CORDON)).send();
    defer cordon.deinit();
    try cordon.expectStatus(.ok);
    try std.testing.expectEqual(@as(i64, 1), try eventCount(conn, runner_id, .runner_cordoned));

    const ep = try eventsPath(runner_id);
    defer ALLOC.free(ep);
    const events = try (try h.get(ep).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer events.deinit();
    try events.expectStatus(.ok);
    try std.testing.expect(events.bodyContains("\"runner_registered\""));
    try std.testing.expect(events.bodyContains("\"runner_cordoned\""));
    try std.testing.expect(events.bodyContains("\"total\":2"));
}

test "integration: lease and report append acquire and release events" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupFleetWork(h, conn);

    try seedFleetWork(conn);
    try publishFreshEvent(h);

    const lease = try leaseOnce(h);
    defer freeLease(lease);
    try std.testing.expectEqual(@as(i64, 1), try eventCount(conn, RUNNER_ID, .lease_acquired));

    const report = try reportLease(h, lease);
    defer report.deinit();
    try report.expectStatus(.ok);
    try std.testing.expectEqual(@as(i64, 1), try eventCount(conn, RUNNER_ID, .lease_released));

    const ep = try eventsPath(RUNNER_ID);
    defer ALLOC.free(ep);
    const events = try (try h.get(ep).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer events.deinit();
    try events.expectStatus(.ok);
    try std.testing.expect(events.bodyContains("\"lease_acquired\""));
    try std.testing.expect(events.bodyContains("\"lease_released\""));
    try std.testing.expect(events.bodyContains("\"total\":2"));

    // The retired page-number spelling is refused, never silently ignored.
    const beyond_page = try eventsPathWithQuery(RUNNER_ID, "page=2&page_size=10");
    defer ALLOC.free(beyond_page);
    const beyond_events = try (try h.get(beyond_page).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer beyond_events.deinit();
    try beyond_events.expectStatus(.bad_request);
    try std.testing.expect(beyond_events.bodyContains("UZ-REQ-001"));

    const last_busy = try eventsPathWithQuery(RUNNER_ID, "event_type=lease_acquired&since=0&limit=1");
    defer ALLOC.free(last_busy);
    const busy_events = try (try h.get(last_busy).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer busy_events.deinit();
    try busy_events.expectStatus(.ok);
    try std.testing.expect(busy_events.bodyContains("\"lease_acquired\""));
    try std.testing.expect(busy_events.bodyContains("\"total\":1"));

    const empty_window = try eventsPathWithQuery(RUNNER_ID, "event_type=lease_acquired&until=0&limit=10");
    defer ALLOC.free(empty_window);
    const no_events = try (try h.get(empty_window).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer no_events.deinit();
    try no_events.expectStatus(.ok);
    try std.testing.expect(no_events.bodyContains("\"total\":0"));
}

test "integration: test_runner_events_accepts_comma_separated_type_set" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupRegister(conn);

    const register = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer register.deinit();
    try register.expectStatus(.created);
    const runner_id = try registeredRunnerId(conn);
    defer ALLOC.free(runner_id);

    const p = try patchPath(runner_id);
    defer ALLOC.free(p);
    const cordon = try (try (try h.request(.PATCH, p).bearer(PLATFORM_ADMIN_TOKEN)).json(BODY_CORDON)).send();
    defer cordon.deinit();
    try cordon.expectStatus(.ok);
    const drain = try (try (try h.request(.PATCH, p).bearer(PLATFORM_ADMIN_TOKEN)).json("{\"action\":\"drain\"}")).send();
    defer drain.deinit();
    try drain.expectStatus(.ok);

    // Three event types exist; the two-tag set returns exactly their union.
    const ep = try eventsPathWithQuery(runner_id, "event_type=runner_registered,runner_cordoned&limit=10");
    defer ALLOC.free(ep);
    const events = try (try h.get(ep).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer events.deinit();
    try events.expectStatus(.ok);
    try std.testing.expect(events.bodyContains("\"runner_registered\""));
    try std.testing.expect(events.bodyContains("\"runner_cordoned\""));
    try std.testing.expect(!events.bodyContains("\"runner_draining\""));
    try std.testing.expect(events.bodyContains("\"total\":2"));
}

test "integration: test_runner_events_single_value_filter_unchanged" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupRegister(conn);

    const register = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer register.deinit();
    try register.expectStatus(.created);
    const runner_id = try registeredRunnerId(conn);
    defer ALLOC.free(runner_id);

    const p = try patchPath(runner_id);
    defer ALLOC.free(p);
    const cordon = try (try (try h.request(.PATCH, p).bearer(PLATFORM_ADMIN_TOKEN)).json(BODY_CORDON)).send();
    defer cordon.deinit();
    try cordon.expectStatus(.ok);

    const ep = try eventsPathWithQuery(runner_id, "event_type=runner_cordoned&limit=10");
    defer ALLOC.free(ep);
    const events = try (try h.get(ep).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer events.deinit();
    try events.expectStatus(.ok);
    try std.testing.expect(events.bodyContains("\"runner_cordoned\""));
    try std.testing.expect(!events.bodyContains("\"runner_registered\""));
    try std.testing.expect(events.bodyContains("\"total\":1"));
}

test "integration: test_runner_events_rejects_unknown_type_in_set" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupRegister(conn);

    const register = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer register.deinit();
    try register.expectStatus(.created);
    const runner_id = try registeredRunnerId(conn);
    defer ALLOC.free(runner_id);

    const ep = try eventsPathWithQuery(runner_id, "event_type=runner_online,not_a_type&limit=10");
    defer ALLOC.free(ep);
    const events = try (try h.get(ep).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer events.deinit();
    try events.expectStatus(.bad_request);
    try std.testing.expect(events.bodyContains("UZ-REQ-001"));
    try std.testing.expect(!events.bodyContains("\"items\""));
}

test "integration: test_runner_events_rejects_empty_type_parameter" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupRegister(conn);

    const register = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer register.deinit();
    try register.expectStatus(.created);
    const runner_id = try registeredRunnerId(conn);
    defer ALLOC.free(runner_id);

    // An empty value must refuse, never silently mean "all".
    const ep = try eventsPathWithQuery(runner_id, "event_type=&limit=10");
    defer ALLOC.free(ep);
    const events = try (try h.get(ep).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer events.deinit();
    try events.expectStatus(.bad_request);
    try std.testing.expect(events.bodyContains("UZ-REQ-001"));
}

test "integration: heartbeat keeps liveness update when runner event insert fails" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupFleetWork(h, conn);
    defer dropHeartbeatEventRejector(conn);

    try seedRunner(conn);
    try installHeartbeatEventRejector(conn);
    try std.testing.expectEqual(protocol.RUNNER_LAST_SEEN_NEVER, try runnerLastSeen(conn, RUNNER_ID));

    const heartbeat = try (try h.post(protocol.PATH_RUNNER_HEARTBEATS).bearer(RUNNER_TOKEN)).rawBody("").send();
    defer heartbeat.deinit();
    try heartbeat.expectStatus(.ok);

    try std.testing.expect((try runnerLastSeen(conn, RUNNER_ID)) > protocol.RUNNER_LAST_SEEN_NEVER);
    try std.testing.expectEqual(@as(i64, 0), try eventCount(conn, RUNNER_ID, .runner_online));
}

test "integration: delete lifecycle - 409 while live, 204 once revoked, cascade clears events, then 404" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupRegister(conn);

    const register = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(REGISTER_BODY)).send();
    defer register.deinit();
    try register.expectStatus(.created);
    const runner_id = try registeredRunnerId(conn);
    defer ALLOC.free(runner_id);

    const p = try patchPath(runner_id);
    defer ALLOC.free(p);

    // Live runner: the revoke-first guard must refuse, and refuse with the
    // registered conflict code — not a bare 409.
    const premature = try (try h.request(.DELETE, p).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer premature.deinit();
    try premature.expectStatus(.conflict);
    try std.testing.expect(premature.bodyContains("UZ-RUN-016"));
    try std.testing.expectEqual(@as(i64, 1), try eventCount(conn, runner_id, .runner_registered));

    const revoke = try (try (try h.request(.PATCH, p).bearer(PLATFORM_ADMIN_TOKEN)).json("{\"action\":\"revoke\"}")).send();
    defer revoke.deinit();
    try revoke.expectStatus(.ok);

    const deleted = try (try h.request(.DELETE, p).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer deleted.deinit();
    try deleted.expectStatus(.no_content);

    // The row is gone and the cascade took the event history with it — the FK
    // on fleet.runner_events is ON DELETE CASCADE and runs as constraint owner,
    // so the append-only privilege posture never had to be widened.
    try std.testing.expectEqual(@as(i64, 0), try runnerRowCount(conn, runner_id));
    try std.testing.expectEqual(@as(i64, 0), try eventCount(conn, runner_id, .runner_registered));
    try std.testing.expectEqual(@as(i64, 0), try eventCount(conn, runner_id, .runner_revoked));

    // Idempotence at the HTTP layer: the second delete is a clean 404, distinct
    // from the pre-revoke 409.
    const again = try (try h.request(.DELETE, p).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer again.deinit();
    try again.expectStatus(.not_found);
    try std.testing.expect(again.bodyContains("UZ-RUN-014"));
}

// ── Keyset paging over the events read ──────────────────────────────────────

fn seedRunnerEventRow(conn: anytype, runner_id: []const u8, suffix: []const u8, event_type: []const u8, occurred_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_events (id, runner_id, event_type, metadata, dedup_key, created_at)
        \\VALUES (overlay(md5($1 || $2)::uuid::text placing '7' from 15 for 1)::uuid,
        \\        $1::uuid, $3, '{}'::jsonb, NULL, $4)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, suffix, event_type, occurred_at });
}

fn cleanupSeededRunner(conn: anytype) void {
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID}) catch |err|
        std.log.warn("cleanup seeded runner ignored: {s}", .{@errorName(err)});
}

/// Walk the events read to exhaustion under `query_prefix` (filters), asserting
/// every non-final page is `limit` long; returns ids in arrival order.
fn walkEvents(h: *TestHarness, query_prefix: []const u8, limit: usize) !std.ArrayList([]const u8) {
    var ids: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (ids.items) |id| ALLOC.free(id);
        ids.deinit(ALLOC);
    }
    var cursor: ?[]const u8 = null;
    defer if (cursor) |c| ALLOC.free(c);
    while (true) {
        const query = if (cursor) |c|
            try std.fmt.allocPrint(ALLOC, "{s}limit={d}&starting_after={s}", .{ query_prefix, limit, c })
        else
            try std.fmt.allocPrint(ALLOC, "{s}limit={d}", .{ query_prefix, limit });
        defer ALLOC.free(query);
        const path = try eventsPathWithQuery(RUNNER_ID, query);
        defer ALLOC.free(path);
        const resp = try (try h.get(path).bearer(PLATFORM_ADMIN_TOKEN)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
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

fn freeWalked(ids: *std.ArrayList([]const u8)) void {
    for (ids.items) |id| ALLOC.free(id);
    ids.deinit(ALLOC);
}

test "integration: test_runner_events_uses_keyset_envelope" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupSeededRunner(conn);
    try seedRunner(conn);
    try seedRunnerEventRow(conn, RUNNER_ID, "env-1", "runner_online", 1_700_000_000_000);

    const ep = try eventsPathWithQuery(RUNNER_ID, "limit=10");
    defer ALLOC.free(ep);
    const resp = try (try h.get(ep).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    try std.testing.expectEqual(@as(usize, 3), obj.count());
    try std.testing.expect(obj.contains("items"));
    try std.testing.expect(obj.contains("total"));
    try std.testing.expect(obj.contains("next_cursor"));

    // page_size is retired: refused, never silently ignored.
    const retired = try eventsPathWithQuery(RUNNER_ID, "page_size=10");
    defer ALLOC.free(retired);
    const refused = try (try h.get(retired).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer refused.deinit();
    try refused.expectStatus(.bad_request);
    try std.testing.expect(refused.bodyContains("UZ-REQ-001"));

    // A structurally valid cursor whose id half is not a uuid still seeks a
    // ::uuid bind, so it is refused at parse — never a cast error's 500.
    const bad_cursor = try eventsPathWithQuery(RUNNER_ID, "starting_after=1744000000000:not-a-uuid");
    defer ALLOC.free(bad_cursor);
    const bad = try (try h.get(bad_cursor).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer bad.deinit();
    try bad.expectStatus(.bad_request);
    try std.testing.expect(bad.bodyContains("UZ-REQ-001"));
}

test "integration: test_runner_events_same_millisecond_rows_are_not_skipped" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupSeededRunner(conn);
    try seedRunner(conn);
    const shared_ms: i64 = 1_700_000_100_000;
    const suffixes = [_][]const u8{ "ms-1", "ms-2", "ms-3", "ms-4", "ms-5" };
    for (suffixes) |suffix| {
        try seedRunnerEventRow(conn, RUNNER_ID, suffix, "runner_online", shared_ms);
    }

    var ids = try walkEvents(h, "", 2);
    defer freeWalked(&ids);
    try std.testing.expectEqual(@as(usize, 5), ids.items.len);
    for (ids.items, 0..) |a, i| {
        for (ids.items[i + 1 ..]) |b| try std.testing.expect(!std.mem.eql(u8, a, b));
    }
}

test "integration: test_runner_events_type_filter_survives_keyset_paging" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupSeededRunner(conn);
    try seedRunner(conn);
    const base_ms: i64 = 1_700_000_200_000;
    try seedRunnerEventRow(conn, RUNNER_ID, "flt-on-1", "runner_online", base_ms + 1);
    try seedRunnerEventRow(conn, RUNNER_ID, "flt-on-2", "runner_online", base_ms + 2);
    try seedRunnerEventRow(conn, RUNNER_ID, "flt-on-3", "runner_online", base_ms + 3);
    try seedRunnerEventRow(conn, RUNNER_ID, "flt-off-1", "runner_offline", base_ms + 4);
    try seedRunnerEventRow(conn, RUNNER_ID, "flt-off-2", "runner_offline", base_ms + 5);
    try seedRunnerEventRow(conn, RUNNER_ID, "flt-cord-1", "runner_cordoned", base_ms + 6);
    try seedRunnerEventRow(conn, RUNNER_ID, "flt-cord-2", "runner_cordoned", base_ms + 7);

    // The two-tag set holds across every cursored page: five union rows, the
    // cordoned rows never leak in.
    var ids = try walkEvents(h, "event_type=runner_online,runner_offline&", 2);
    defer freeWalked(&ids);
    try std.testing.expectEqual(@as(usize, 5), ids.items.len);

    const full = try eventsPathWithQuery(RUNNER_ID, "event_type=runner_online,runner_offline&limit=10");
    defer ALLOC.free(full);
    const resp = try (try h.get(full).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    try std.testing.expect(!resp.bodyContains("runner_cordoned"));
    try std.testing.expect(resp.bodyContains("\"total\":5"));
}
