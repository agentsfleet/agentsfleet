// Integration tests for the self-test arc across the heartbeat (M167 §2) — the
// operator's request rides the reply down, the daemon's verdict rides the
// request up, and reporting one retires the ask.
//
// Against the live test DB, because the properties that matter are storage
// properties: that a verdict lands on the runner row, that the request is
// one-shot rather than a standing order, and that a refused verdict leaves the
// row untouched WITHOUT failing the liveness beat carrying it. None of those
// are observable from the handler in isolation.
//
// Spec test names carried here (graded at VERIFY):
//   2.x test_heartbeat_stores_a_reported_selftest_verdict
//   2.x test_selftest_request_rides_the_reply_and_is_one_shot
//   2.x test_a_verdict_claiming_false_health_is_refused
//
// Requires TEST_DATABASE_URL (harness skips otherwise). Per the harness rules,
// cleanup helpers acquire their own connection.

const std = @import("std");
const pg = @import("pg");

const serve_runner_lookup = @import("../../../cmd/serve_runner_lookup.zig");
const base = @import("../../../db/test_fixtures.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const protocol = @import("contract").protocol;

const ALLOC = std.testing.allocator;

const PLATFORM_ADMIN_TOKEN = scope_fixtures.PLATFORM_ADMIN;

// Distinct UUIDv7 literals — no collision with sibling runner suites.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fb111";

// One host per test so residue from an aborted run never cross-fires.
const HOST_REPORT = "selftest-hb-host-report";
const HOST_REQUEST = "selftest-hb-host-request";
const HOST_REFUSED = "selftest-hb-host-refused";

// SAFETY: populated by configureRegistry before the runner_bearer middleware
// reads it; the harness wires the registry ahead of the first request.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn registerBody(comptime host: []const u8) []const u8 {
    return "{\"host_id\":\"" ++ host ++ "\",\"assigned_policy\":{\"sandbox_tier\":\"dev_none\"" ++
        ",\"network_policy\":\"allow_all\",\"registry_allowlist\":[],\"worker_count\":1}," ++
        "\"labels\":[]}";
}

const Registered = struct { runner_id: []const u8, runner_token: []const u8 };

fn register(h: *TestHarness, body: []const u8) !Registered {
    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.created);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    const id = try ALLOC.dupe(u8, obj.get("runner_id").?.string);
    errdefer ALLOC.free(id);
    return .{ .runner_id = id, .runner_token = try ALLOC.dupe(u8, obj.get("runner_token").?.string) };
}

fn freeRegistered(r: Registered) void {
    ALLOC.free(r.runner_id);
    ALLOC.free(r.runner_token);
}

fn heartbeat(h: *TestHarness, token: []const u8, body: []const u8) !harness_mod.Response {
    return (try (try h.post(protocol.PATH_RUNNER_HEARTBEATS).bearer(token)).json(body)).send();
}

/// The stored self-test columns, duped for the caller.
const SelftestRow = struct {
    has_checks: bool,
    all_ok: ?bool,
    tier: ?[]u8,
    completed_at: i64,
    requested: bool,

    fn deinit(self: SelftestRow) void {
        if (self.tier) |t| ALLOC.free(t);
    }
};

fn selftestRow(conn: *pg.Conn, runner_id: []const u8) !SelftestRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT selftest_checks IS NOT NULL, selftest_all_ok, selftest_sandbox_tier,
        \\       COALESCE(selftest_completed_at, 0), selftest_requested_at IS NOT NULL
        \\FROM fleet.runners WHERE id = $1::uuid
    , .{runner_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const tier_raw = try row.get(?[]const u8, 2);
    return .{
        .has_checks = try row.get(bool, 0),
        .all_ok = try row.get(?bool, 1),
        .tier = if (tier_raw) |t| try ALLOC.dupe(u8, t) else null,
        .completed_at = try row.get(i64, 3),
        .requested = try row.get(bool, 4),
    };
}

fn requestSelftest(conn: *pg.Conn, runner_id: []const u8) void {
    _ = conn.exec(
        "UPDATE fleet.runners SET selftest_requested_at = 1 WHERE id = $1::uuid",
        .{runner_id},
    ) catch |err| std.log.warn("selftest request seed ignored: {s}", .{@errorName(err)});
}

fn replyAsksForSelftest(body: []const u8) !bool {
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, body, .{});
    defer parsed.deinit();
    const v = parsed.value.object.get("selftest_requested") orelse return false;
    return v.bool;
}

fn execIgnore(conn: *pg.Conn, comptime q: []const u8, args: anytype) void {
    _ = conn.exec(q, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanupAll(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    execIgnore(conn,
        \\DELETE FROM fleet.runners WHERE host_id IN ($1, $2, $3)
    , .{ HOST_REPORT, HOST_REQUEST, HOST_REFUSED });
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

const HB_EMPTY = "{}";

// A well-formed verdict: one failing check, `all_ok` false, and the policy it
// ran under. The failing arm is the interesting one — a green verdict proves
// storage, a red one proves the row can carry the fault an operator must see.
const HB_VERDICT_FAILING =
    \\{"selftest":{"checks":[
    \\  {"name":"resolver file resolves inside the sandbox","ok":false,
    \\   "detail":"/etc/resolv.conf does not resolve to a readable file"}],
    \\ "all_ok":false,"sandbox_tier":"dev_none","network_policy":"allow_all"}}
;

// `all_ok` true while a check failed — a runner claiming health its own checks
// contradict, which is the exact shape of the incident M167 exists to close.
const HB_VERDICT_LYING =
    \\{"selftest":{"checks":[
    \\  {"name":"resolver file resolves inside the sandbox","ok":false,
    \\   "detail":"/etc/resolv.conf does not resolve to a readable file"}],
    \\ "all_ok":true,"sandbox_tier":"dev_none","network_policy":"allow_all"}}
;

test "integration: test_heartbeat_stores_a_reported_selftest_verdict" {
    const h = try startHarness();
    defer h.deinit();
    defer cleanupAll(h);
    cleanupAll(h);

    const reg = try register(h, registerBody(HOST_REPORT));
    defer freeRegistered(reg);

    {
        const resp = try heartbeat(h, reg.runner_token, HB_VERDICT_FAILING);
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const row = try selftestRow(conn, reg.runner_id);
    defer row.deinit();

    try std.testing.expect(row.has_checks);
    try std.testing.expectEqual(@as(?bool, false), row.all_ok);
    try std.testing.expect(row.completed_at > 0);
    // The policy travels WITH the verdict so a later re-assignment renders it
    // stale rather than as a verdict on the new policy (Dimension 1.3).
    try std.testing.expectEqualStrings("dev_none", row.tier.?);
}

test "integration: test_selftest_request_rides_the_reply_and_is_one_shot" {
    const h = try startHarness();
    defer h.deinit();
    defer cleanupAll(h);
    cleanupAll(h);

    const reg = try register(h, registerBody(HOST_REQUEST));
    defer freeRegistered(reg);

    // No request outstanding: the reply must not ask for a probe nobody wants.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(!try replyAsksForSelftest(resp.body));
    }

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        requestSelftest(conn, reg.runner_id);
    }

    // The ask reaches the host on the next beat — no second endpoint, no poll.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(try replyAsksForSelftest(resp.body));
    }

    // Reporting retires it. The same beat must NOT echo the request back, or the
    // host would immediately re-run the probe it just finished — and the row's
    // request must be cleared, or it would re-run on every beat forever.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_VERDICT_FAILING);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(!try replyAsksForSelftest(resp.body));
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const row = try selftestRow(conn, reg.runner_id);
        defer row.deinit();
        try std.testing.expect(!row.requested);
        try std.testing.expect(row.has_checks);
    }

    // And a later beat does not resurrect the ask.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(!try replyAsksForSelftest(resp.body));
    }
}

test "integration: test_a_verdict_claiming_false_health_is_refused" {
    const h = try startHarness();
    defer h.deinit();
    defer cleanupAll(h);
    cleanupAll(h);

    const reg = try register(h, registerBody(HOST_REFUSED));
    defer freeRegistered(reg);

    // The beat still succeeds: a runner token must not be able to fail its own
    // liveness by reporting nonsense, or a bad verdict would look like a dead
    // host and cost the fleet a runner.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_VERDICT_LYING);
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }

    // But nothing was stored — the row must not carry a verdict nobody can
    // trust, which is how a broken host keeps reading healthy.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const row = try selftestRow(conn, reg.runner_id);
    defer row.deinit();
    try std.testing.expect(!row.has_checks);
    try std.testing.expectEqual(@as(?bool, null), row.all_ok);
    try std.testing.expectEqual(@as(i64, 0), row.completed_at);
}
