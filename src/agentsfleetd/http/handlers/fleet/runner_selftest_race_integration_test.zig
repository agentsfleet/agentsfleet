//! Integration tier for the two arms that only fire when the runner row changes
//! under a self-test ask: `RunnerGone` and `RevokedRace`.
//!
//! `innerPatchFleetRunner` reads the runner's state, then `requestSelfTest`
//! stamps it behind a guard that refuses a revoked row. Both arms need the row
//! to disagree with what that read returned, which no single HTTP request can
//! arrange — so over the route they are unreachable, and the sibling
//! `runner_selftest_patch_integration_test.zig` says so in its header.
//!
//! Called directly they need no race whatsoever. The arms do not test for a
//! race; they test what the guard found when it rejected the write, and a row
//! deleted or revoked BEFORE the call reproduces exactly that state. Staging a
//! real race to reach a deterministic branch would buy flakiness and nothing
//! else.
//!
//! What the arms are for: an operator whose ask silently did nothing cannot
//! tell a vanished runner from a revoked one, and those are different problems
//! with different fixes. The distinction is the whole reason the row is re-read
//! rather than the write simply returning "no".
//!
//! The second half asserts what `applySelfTestRequest` MAKES of each refusal —
//! the HTTP reply the operator actually sees. The response side rides
//! `httpz.testing` (the `hx_test.zig` pattern); the database side stays real:
//! staged rows for the two race arms, and for the else arm a transaction
//! poisoned on purpose, which is a genuine `pg.Conn` refusing work on command.
//!
//! Needs `TEST_DATABASE_URL` AND `REDIS_URL_API` — the harness skips without
//! either, so `make test-integration-db` (no Redis env) SKIPS this file;
//! `make test-integration` runs it. Self-skips otherwise.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");
const httpz = @import("httpz");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const ec = @import("../../../errors/error_registry.zig");
const hx_mod = @import("../hx.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const runner_patch = @import("runner_patch.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;
const Hx = hx_mod.Hx;

// Fixed v7 ids (the `7` nibble opens the third group), distinct from every
// sibling file's so a parallel lane cannot delete a row this one is using.
const RUNNER_VANISHED = "0195b4ba-8d3a-7f13-8abc-00000000ec01";
const RUNNER_REVOKED = "0195b4ba-8d3a-7f13-8abc-00000000ec02";
const RUNNER_LIVE = "0195b4ba-8d3a-7f13-8abc-00000000ec03";
const RUNNER_MAPPED_GONE = "0195b4ba-8d3a-7f13-8abc-00000000ec04";
const RUNNER_MAPPED_REVOKED = "0195b4ba-8d3a-7f13-8abc-00000000ec05";
const RUNNER_FAULTED_CONN = "0195b4ba-8d3a-7f13-8abc-00000000ec06";
const HOST_PREFIX = "selftest-race-";

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

/// Best-effort teardown: a failed delete must not mask the assertion the test
/// just made, and the next seed upserts over any survivor.
fn deleteRunner(conn: *pg.Conn, id: []const u8) void {
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{id}) catch |err|
        std.log.warn("selftest race fixture cleanup ignored: {s}", .{@errorName(err)});
}

test "integration: a runner that vanished under the ask is named, not swallowed" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    // Seeded then removed: the state the handler's read saw is gone by the time
    // the stamp runs, which is precisely the condition the arm exists for.
    try seedRunner(conn, RUNNER_VANISHED, "active");
    deleteRunner(conn, RUNNER_VANISHED);

    try std.testing.expectError(
        error.RunnerGone,
        runner_patch.requestSelfTest(conn, RUNNER_VANISHED, clock.nowMillis()),
    );
}

test "integration: a runner revoked under the ask reports the revocation, not a lost row" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_REVOKED);

    // The row EXISTS and is revoked. The guard refuses the write for a reason
    // the operator can act on, and telling this apart from a vanished runner is
    // the only reason the row is re-read after the guard rejects.
    try seedRunner(conn, RUNNER_REVOKED, "revoked");

    try std.testing.expectError(
        error.RevokedRace,
        runner_patch.requestSelfTest(conn, RUNNER_REVOKED, clock.nowMillis()),
    );
}

test "integration: an unguarded row stamps the instant it was asked" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_LIVE);
    try seedRunner(conn, RUNNER_LIVE, "active");

    // The success arm, asserted beside the refusals so a `requestSelfTest` that
    // refused EVERYTHING would fail here rather than read as two passing guards.
    const asked = clock.nowMillis();
    const recorded = try runner_patch.requestSelfTest(conn, RUNNER_LIVE, asked);
    try std.testing.expectEqual(asked, recorded);

    var row = (try conn.row("SELECT selftest_requested_at FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_LIVE})) orelse
        return error.RunnerMissing;
    defer row.deinit() catch |err|
        std.log.warn("selftest race row deinit ignored: {s}", .{@errorName(err)});
    try std.testing.expectEqual(@as(?i64, asked), try row.get(?i64, 0));
}

fn buildHx(res: *httpz.Response, req_id: []const u8) Hx {
    return Hx{
        .alloc = std.testing.allocator,
        // The failure arms only touch res + req_id — if that changes, this
        // test crashes and surfaces the coupling.
        // SAFETY: test fixture; never read by the arms under test.
        .principal = undefined,
        .req_id = req_id,
        // SAFETY: test fixture; never read by the arms under test.
        .ctx = undefined,
        .res = res,
    };
}

test "integration: a vanished runner surfaces to the operator as 404 runner-not-found" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    try seedRunner(conn, RUNNER_MAPPED_GONE, "active");
    deleteRunner(conn, RUNNER_MAPPED_GONE);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    runner_patch.applySelfTestRequest(buildHx(ht.res, "req-selftest-gone"), conn, RUNNER_MAPPED_GONE, .active);

    try ht.expectStatus(404);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings(ec.ERR_RUNNER_NOT_FOUND, json.object.get("error_code").?.string);
}

test "integration: a runner revoked under the ask surfaces as the self-test refusal, not a lost row" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_MAPPED_REVOKED);

    // `current` is what the handler's read returned BEFORE the revocation
    // landed — the exact half-stale view the race arm exists to answer.
    try seedRunner(conn, RUNNER_MAPPED_REVOKED, "revoked");

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    runner_patch.applySelfTestRequest(buildHx(ht.res, "req-selftest-revoked"), conn, RUNNER_MAPPED_REVOKED, .active);

    try ht.expectStatus(409);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings(ec.ERR_RUN_SELFTEST_REFUSED, json.object.get("error_code").?.string);
}

test "integration: a connection that refuses the write surfaces as a 500, never a silent ok" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    // A transaction poisoned on purpose: after the failed statement Postgres
    // refuses every command on this connection until ROLLBACK. That is a real
    // `pg.Conn` failing on demand — no seam, no double.
    _ = try conn.exec("BEGIN", .{});
    try std.testing.expectError(error.PG, conn.exec("SELECT 1/0", .{}));

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    runner_patch.applySelfTestRequest(buildHx(ht.res, "req-selftest-conn-fault"), conn, RUNNER_FAULTED_CONN, .active);

    // Best-effort un-poison. The errored write can leave the wire mid-result,
    // in which case this rollback is refused (`ConnectionBusy`) — harmless:
    // `pool.release` destroys and replaces any non-idle connection.
    _ = conn.exec("ROLLBACK", .{}) catch |err|
        std.log.warn("selftest fault rollback ignored: {s}", .{@errorName(err)});

    try ht.expectStatus(500);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings(ec.ERR_INTERNAL_DB_QUERY, json.object.get("error_code").?.string);
}
