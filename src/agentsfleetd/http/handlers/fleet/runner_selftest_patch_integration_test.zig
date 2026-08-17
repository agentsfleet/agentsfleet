//! Integration tier for the operator-plane self-test ask:
//! `PATCH /v1/fleets/runners/{id}` with `{"action":"self_test"}`.
//!
//! The arm had no daemon-side test at all — the request path, its refusal, and
//! the guard that makes a revoked runner refuse were asserted by nothing. What
//! matters to an operator is that the reply is the recorded REQUEST and never a
//! verdict: the daemon collects the ask on its next heartbeat and answers on a
//! later one, so a synchronous wait here would hang the dashboard on exactly
//! the offline host an operator most wants to test.
//!
//! The two race arms inside `requestSelfTest` (`RunnerGone`, `RevokedRace`) are
//! reachable only when the row changes between `loadState` and the guarded
//! UPDATE. Rather than stage a flaky race, the guard those arms depend on is
//! proven directly against the statement at the bottom of this file.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");
const protocol = @import("contract").protocol;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const ec = @import("../../../errors/error_registry.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const sql = @import("sql.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;

/// PLATFORM_ADMIN carries runner:enroll + runner:read + runner:write — the
/// self-test ask is gated on runner:write, the same scope as the transitions.
const TOKEN_OPERATOR = scope_fixtures.PLATFORM_ADMIN;

// Fixed v7 ids (the `7` nibble opens the third group) — the handler refuses a
// non-v7 runner id before it reaches the action fork.
const RUNNER_ACTIVE = "0195b4ba-8d3a-7f13-8abc-00000000e001";
const RUNNER_REVOKED = "0195b4ba-8d3a-7f13-8abc-00000000e002";
const RUNNER_ABSENT = "0195b4ba-8d3a-7f13-8abc-00000000e0ff";
const HOST_PREFIX = "selftest-patch-";

const BODY_SELF_TEST = "{\"action\":\"self_test\"}";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedRunner(conn: *pg.Conn, id: []const u8, admin_state: []const u8) !void {
    const now = clock.nowMillis();
    var host_buf: [64]u8 = undefined;
    const host = try std.fmt.bufPrint(&host_buf, "{s}{s}", .{ HOST_PREFIX, id[24..] });
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $2, 'standard', $3, '[]'::jsonb, $4, $4, $4)
        \\ON CONFLICT (id) DO UPDATE
        \\  SET admin_state = EXCLUDED.admin_state, selftest_requested_at = NULL
    , .{ id, host, admin_state, now });
}

/// Fixture teardown. Best-effort by design: a failed delete must not mask the
/// assertion the test just made, and the next run's seed upserts over any
/// survivor. Mirrors `execIgnore` in the heartbeat sister file.
fn deleteRunner(conn: *pg.Conn, id: []const u8) void {
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{id}) catch |err|
        std.log.warn("selftest patch fixture cleanup ignored: {s}", .{@errorName(err)});
}

/// The stamped ask, or null when the row carries none.
fn requestedAt(conn: *pg.Conn, id: []const u8) !?i64 {
    var row = (try conn.row("SELECT selftest_requested_at FROM fleet.runners WHERE id = $1::uuid", .{id})) orelse
        return error.RunnerMissing;
    defer row.deinit() catch |err|
        std.log.warn("selftest patch row deinit ignored: {s}", .{@errorName(err)});
    return row.get(?i64, 0);
}

fn patchUrl(alloc: std.mem.Allocator, id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/fleets/runners/{s}", .{id});
}

test "integration: an operator's self-test ask is recorded and answers with the request, not a verdict" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_ACTIVE);
    try seedRunner(conn, RUNNER_ACTIVE, @tagName(protocol.AdminState.active));

    const before = clock.nowMillis();
    const url = try patchUrl(alloc, RUNNER_ACTIVE);
    defer alloc.free(url);

    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_OPERATOR)).json(BODY_SELF_TEST)).send();
    defer r.deinit();
    try std.testing.expectEqual(@as(u16, 200), r.status);

    // The reply carries the recorded ask. A verdict here would be a lie — the
    // host has not been contacted yet.
    try std.testing.expect(std.mem.indexOf(u8, r.body, "selftest_requested_at") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"checks\"") == null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "all_ok") == null);

    const stamped = (try requestedAt(conn, RUNNER_ACTIVE)) orelse return error.TestUnexpectedResult;
    try std.testing.expect(stamped >= before);
}

test "integration: a revoked runner refuses the ask — it will never heartbeat again to answer" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_REVOKED);
    try seedRunner(conn, RUNNER_REVOKED, @tagName(protocol.AdminState.revoked));

    const url = try patchUrl(alloc, RUNNER_REVOKED);
    defer alloc.free(url);

    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_OPERATOR)).json(BODY_SELF_TEST)).send();
    defer r.deinit();

    const refused = ec.lookup(ec.ERR_RUN_SELFTEST_REFUSED);
    try std.testing.expectEqual(@intFromEnum(refused.http_status), @as(u16, r.status));
    try std.testing.expect(std.mem.indexOf(u8, r.body, refused.code) != null);

    // Refused means NOT recorded: a stamped ask nobody can answer would leave
    // the page reading "pending" forever.
    try std.testing.expectEqual(@as(?i64, null), try requestedAt(conn, RUNNER_REVOKED));
}

test "integration: test_selftest_control_requires_write_scope" {
    // Dimension 1.4's daemon half. The dashboard withholds the control from a
    // read-only operator (RunnerHeader.selftest.test.tsx), but the arm is
    // reachable directly, so the refusal has to be the route guard's — not the
    // UI's. VIEWER carries fleet:read + schedule:read and no runner scope at
    // all, which is what "without runner:write" means at this boundary.
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_ACTIVE);
    try seedRunner(conn, RUNNER_ACTIVE, @tagName(protocol.AdminState.active));

    const url = try patchUrl(alloc, RUNNER_ACTIVE);
    defer alloc.free(url);

    const r = try (try (try h.request(.PATCH, url).bearer(scope_fixtures.VIEWER)).json(BODY_SELF_TEST)).send();
    defer r.deinit();
    try std.testing.expectEqual(@as(u16, 403), r.status);
    try std.testing.expect(std.mem.indexOf(u8, r.body, ec.ERR_INSUFFICIENT_SCOPE) != null);

    // Refused at the guard means the row is untouched — a scope failure that
    // still stamped the ask would leave the page pending on an unauthorised
    // click.
    try std.testing.expectEqual(@as(?i64, null), try requestedAt(conn, RUNNER_ACTIVE));
}

test "integration: an ask against a runner that does not exist is a 404, never a silent no-op" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        deleteRunner(conn, RUNNER_ABSENT);
    }

    const url = try patchUrl(alloc, RUNNER_ABSENT);
    defer alloc.free(url);

    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_OPERATOR)).json(BODY_SELF_TEST)).send();
    defer r.deinit();
    try std.testing.expectEqual(@as(u16, 404), r.status);
}

test "integration: re-asking while one is outstanding re-stamps rather than refusing" {
    // The control disables itself while a request is pending, but the API is
    // reachable directly and a second ask must not 409 — an operator retrying a
    // host that never answered is the normal case, not an error.
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_ACTIVE);
    try seedRunner(conn, RUNNER_ACTIVE, @tagName(protocol.AdminState.active));

    const url = try patchUrl(alloc, RUNNER_ACTIVE);
    defer alloc.free(url);

    const first = try (try (try h.request(.PATCH, url).bearer(TOKEN_OPERATOR)).json(BODY_SELF_TEST)).send();
    defer first.deinit();
    try std.testing.expectEqual(@as(u16, 200), first.status);
    const first_stamp = (try requestedAt(conn, RUNNER_ACTIVE)) orelse return error.TestUnexpectedResult;

    const second = try (try (try h.request(.PATCH, url).bearer(TOKEN_OPERATOR)).json(BODY_SELF_TEST)).send();
    defer second.deinit();
    try std.testing.expectEqual(@as(u16, 200), second.status);
    const second_stamp = (try requestedAt(conn, RUNNER_ACTIVE)) orelse return error.TestUnexpectedResult;
    try std.testing.expect(second_stamp >= first_stamp);
}

test "integration: the guard behind the race arms refuses to stamp a revoked row" {
    // `requestSelfTest` distinguishes RunnerGone from RevokedRace by re-reading
    // the row after the UPDATE matches nothing. Both arms depend on this WHERE
    // clause matching zero rows for a revoked runner; staging the real race is
    // flaky, so the statement itself is the system under test here.
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_REVOKED);
    try seedRunner(conn, RUNNER_REVOKED, @tagName(protocol.AdminState.revoked));

    var q = try conn.query(sql.PATCH_RUNNER_SELFTEST_REQUEST, .{
        RUNNER_REVOKED,
        clock.nowMillis(),
        @tagName(protocol.AdminState.revoked),
    });
    defer q.deinit();
    try std.testing.expectEqual(@as(?pg.Row, null), try q.next());
    try q.drain();

    // And the same statement DOES stamp an active row, so the zero-row result
    // above is the guard firing rather than the statement never matching.
    defer deleteRunner(conn, RUNNER_ACTIVE);
    try seedRunner(conn, RUNNER_ACTIVE, @tagName(protocol.AdminState.active));
    var ok_q = try conn.query(sql.PATCH_RUNNER_SELFTEST_REQUEST, .{
        RUNNER_ACTIVE,
        clock.nowMillis(),
        @tagName(protocol.AdminState.revoked),
    });
    defer ok_q.deinit();
    try std.testing.expect((try ok_q.next()) != null);
    try ok_q.drain();
}
