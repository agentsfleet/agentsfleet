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
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const runner_patch = @import("runner_patch.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;

// Fixed v7 ids (the `7` nibble opens the third group), distinct from every
// sibling file's so a parallel lane cannot delete a row this one is using.
const RUNNER_VANISHED = "0195b4ba-8d3a-7f13-8abc-00000000ec01";
const RUNNER_REVOKED = "0195b4ba-8d3a-7f13-8abc-00000000ec02";
const RUNNER_LIVE = "0195b4ba-8d3a-7f13-8abc-00000000ec03";
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
