// Runner event history over the live HTTP surface.

const std = @import("std");
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

const TEST_ISSUER = "https://clerk.test.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const REGISTER_HOST = "host-event-register-test";
const REGISTER_BODY =
    \\{"host_id":"host-event-register-test","sandbox_tier":"dev_none","labels":[]}
;
const BODY_CORDON = "{\"action\":\"cordon\"}";

const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6c01";
const SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e6d01";
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

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"qXJuc_Hncnu-ZAFKPEhb6qeXXSp1GcUidOyyiyFFwi5bmql2NZH4Quv23LhHsAKM8L5950bvTQppdzcJ8zWQKx9F8kViZgaG1Ghagoz2a2BMjeSHLFu_gfsxP6y752WUcZ1uHUGnWm9WsDE7xMfbOOpcUoOc_RxiRhwuXjR3zw6J8Vl4DABKQXq_jb6l5nyDWOsi9FopsaS6FKpQoiWO4DWHEHVVNA7RxoYtb1ew9u4qSq4dyeyb6sOXBWuc9wOjSXcuEm30qYsvZJ8ORSh1hxdDaArUCXQKp_DPVJBO7Mmu_EAnOcSsFeZ-kgLVD7yJp_Yq983-s9odwX0TxlL8Lw","e":"AQAB","kid":"m80005-test-kid","use":"sig","alg":"RS256"}]}
;
const PLATFORM_ADMIN_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im04MDAwNS10ZXN0LWtpZCJ9.eyJzdWIiOiJ1c2VyX204MDAwNSIsImlzcyI6Imh0dHBzOi8vY2xlcmsudGVzdC5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJleHAiOjQxMDI0NDQ4MDAsIm1ldGFkYXRhIjp7InRlbmFudF9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYwMSIsIndvcmtzcGFjZV9pZCI6IjAxOTViNGJhLThkM2EtN2YxMy04YWJjLTJiM2UxZTBhNmYxMSIsInJvbGUiOiJhZG1pbiIsInBsYXRmb3JtX2FkbWluIjp0cnVlfX0.H3gZWcqBWYnREFPQAbnoIzhV33ckaYyo37clfhGekxy4TMM96QuUbeyHJW0CnuMRS6UueCjwiidW3mfkINdfQy6-Y4aERoqPvfYQ7QGiwMSPU63heJKxS4fzHzbdDMfO1XoAEcj333xJ8NyvkdBXEbKS9k0LA2-4mczKXLnWkEHnAfWslsK1hdLdIf4rNYP4KahrV25QU-8RirkUTV5jUUgH3HuPMTF976FZX_Q6pL2vW6i1iS2S4iMVwmBdBPlMCPLfjc3Yi9EIP0eBCkWCwZrp1nD5U74Akb6Yh4LCJw9xbhj4kI4jr9e-zwOh7FH_fzbxgJUxHg2jl9pLSAofGw";

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
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'runner-events-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
    try seedRunner(conn);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "runner-events-fleet", CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, SESSION_ID, FLEET_ID, "{}");
}

fn publishFreshEvent(h: *TestHarness) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, FLEET_ID);
    const id = try h.queue.xaddFleetEvent(.{
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
    return std.fmt.allocPrint(ALLOC, "{s}/{s}/events?page=1&page_size=10", .{ protocol.PATH_FLEET_RUNNERS, runner_id });
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

fn cleanupRegister(conn: anytype) void {
    _ = conn.exec("DELETE FROM fleet.runners WHERE host_id = $1", .{REGISTER_HOST}) catch |err|
        std.log.warn("cleanup registered runner ignored: {s}", .{@errorName(err)});
}

fn cleanupFleetWork(h: *TestHarness, conn: anytype) void {
    var resp = h.queue.command(&.{ "DEL", "fleet:" ++ FLEET_ID ++ ":events" }) catch null;
    if (resp) |*r| r.deinit(h.queue.alloc);
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

test "state writes append runner events and history route lists them" {
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

test "lease and report append acquire and release events" {
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

    const beyond_page = try eventsPathWithQuery(RUNNER_ID, "page=2&page_size=10");
    defer ALLOC.free(beyond_page);
    const beyond_events = try (try h.get(beyond_page).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer beyond_events.deinit();
    try beyond_events.expectStatus(.ok);
    try std.testing.expect(beyond_events.bodyContains("\"items\":[]"));
    try std.testing.expect(beyond_events.bodyContains("\"total\":2"));

    const last_busy = try eventsPathWithQuery(RUNNER_ID, "event_type=lease_acquired&since=0&page=1&page_size=1");
    defer ALLOC.free(last_busy);
    const busy_events = try (try h.get(last_busy).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer busy_events.deinit();
    try busy_events.expectStatus(.ok);
    try std.testing.expect(busy_events.bodyContains("\"lease_acquired\""));
    try std.testing.expect(busy_events.bodyContains("\"total\":1"));

    const empty_window = try eventsPathWithQuery(RUNNER_ID, "event_type=lease_acquired&until=0&page=1&page_size=10");
    defer ALLOC.free(empty_window);
    const no_events = try (try h.get(empty_window).bearer(PLATFORM_ADMIN_TOKEN)).send();
    defer no_events.deinit();
    try no_events.expectStatus(.ok);
    try std.testing.expect(no_events.bodyContains("\"total\":0"));
}

test "heartbeat keeps liveness update when runner event insert fails" {
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
