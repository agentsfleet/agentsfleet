// Integration tests for the dual-row lease renewal (`renewal.renew`): the
// fenced, atomic extension of BOTH fleet.runner_affinity.leased_until AND
// fleet.runner_leases.lease_expires_at. This is the highest-risk invariant of
// the renewal path — if the two rows can diverge, a "renewed" run is still
// reclaimed via the affinity path.
//
// Drives `renewal.renew` directly against seeded fleet rows on a live test-DB
// connection, passing an explicit `now_ms` so the clamped deadline is exact and
// the assertions are deterministic. Covers: both rows advance to the same value;
// a stale fence (a reclaim bumped fencing_seq past the lease) → lost; the hard
// max-runtime cap reached → max_runtime.
//
// Requires LIVE_DB=1. Skipped (error.SkipZigTest) when TEST_DATABASE_URL is unset.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const base = @import("../db/test_fixtures.zig");
const constants = @import("common");
const renewal = @import("renewal.zig");

const ALLOC = std.testing.allocator;

const auth_mw = @import("../auth/middleware/mod.zig");
const MS_PER_SECOND = 1_000;

// These tests drive `renewal.renew` directly against the DB, not through the
// HTTP middleware chain, so no policy wiring is needed — the harness just gives
// us a live pool. A no-op registry config satisfies the required field.
fn noopRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7a01";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7c01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7e01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7f01";
const EVENT_ID = "evt-renew-1"; // matches seedLease; metering rows keyed by it

// A fixed "now" for deterministic clamping. Lease created_at is set relative to
// this per-test so the cap is exercised precisely.
const NOW_MS: i64 = 1_900_000_000_000;

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'renew-host', 'renew-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

fn seedAffinity(conn: *pg.Conn, fencing_seq: i64, leased_until: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET fencing_seq = EXCLUDED.fencing_seq, leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, ZOMBIE_ID, RUNNER_ID, fencing_seq, leased_until });
}

/// Seed an active lease for the runner with a given fencing_token + created_at.
fn seedLease(conn: *pg.Conn, fencing_token: i64, created_at: i64, lease_expires_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-renew-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, $6, $7, 'active', $8, $8)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, ZOMBIE_ID, WORKSPACE_ID, base.TEST_TENANT_ID, fencing_token, lease_expires_at, created_at });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    // Breakdown + ledger rows are keyed by event_id and survive a lease/affinity
    // delete; clear them so a re-seeded case starts the per-event slice counter
    // clean (the affinity counter resets, so a leftover slice_seq would collide).
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM core.zombie_execution_telemetry WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

/// Read both rows' deadlines so a test can assert they moved together. Each
/// query is fully consumed + deinit'd in its own scope BEFORE the next is
/// issued — a `pg.Conn` allows only one open result at a time (ZIG_RULES: never
/// read/write on a conn while a result is open).
fn readDeadlines(conn: *pg.Conn) !struct { lease: i64, affinity: i64 } {
    const lease_until = try readBigint(conn, "SELECT lease_expires_at FROM fleet.runner_leases WHERE id = $1::uuid", LEASE_ID);
    const aff_until = try readBigint(conn, "SELECT leased_until FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", ZOMBIE_ID);
    return .{ .lease = lease_until, .affinity = aff_until };
}

/// Run a single-column bigint lookup bound to `id` and fully drain it before
/// returning, so the next query on the same conn finds no open result.
fn readBigint(conn: *pg.Conn, sql: []const u8, id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}

test "renew extends both the lease row and the affinity slot to the same clamped value" {
    var h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn); // workspaces.tenant_id FK requires the tenant first
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    // Fence holds (token == seq). created_at recent → cap not reached. Both rows
    // start at an about-to-expire deadline.
    try seedAffinity(conn, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(conn, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    defer teardown(conn);

    const outcome = try renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, .{});

    // renewed → both rows advanced to min(now+TTL, created_at+MAX). created_at is
    // recent, so the TTL increment wins and that is the new deadline.
    const want = NOW_MS + constants.LEASE_TTL_MS;
    try std.testing.expectEqual(renewal.RenewOutcome{ .renewed = want }, outcome);
    const after = try readDeadlines(conn);
    try std.testing.expectEqual(want, after.lease);
    try std.testing.expectEqual(want, after.affinity); // the divergence guard
}

test "renew on a lease whose fence was bumped by a reclaim returns lost" {
    var h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn); // workspaces.tenant_id FK requires the tenant first
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    // A reclaim bumped the affinity fence past the lease's token: lease holds
    // token 5, the live slot is at seq 6 → the lease is no longer the holder.
    try seedAffinity(conn, 6, NOW_MS + 30_000);
    try seedLease(conn, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    defer teardown(conn);

    const outcome = try renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, .{});
    try std.testing.expectEqual(renewal.RenewOutcome.lost, outcome);
    // Neither row moved.
    const after = try readDeadlines(conn);
    try std.testing.expectEqual(@as(i64, NOW_MS - MS_PER_SECOND), after.lease);
    try std.testing.expectEqual(@as(i64, NOW_MS + 30_000), after.affinity);
}

test "renew past the hard max-runtime cap returns max_runtime and does not extend" {
    var h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn); // workspaces.tenant_id FK requires the tenant first
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    // Fence holds, but created_at is older than MAX_RUNTIME_MS, so the cap
    // (created_at + MAX) is already <= now → no extension is legal.
    const created = NOW_MS - constants.MAX_RUNTIME_MS - 1_000;
    try seedAffinity(conn, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(conn, 5, created, NOW_MS - MS_PER_SECOND);
    defer teardown(conn);

    const outcome = try renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, .{});
    try std.testing.expectEqual(
        renewal.RenewOutcome{ .max_runtime = created + constants.MAX_RUNTIME_MS },
        outcome,
    );
    // Cap reached → rows untouched.
    const after = try readDeadlines(conn);
    try std.testing.expectEqual(@as(i64, NOW_MS - MS_PER_SECOND), after.lease);
    try std.testing.expectEqual(@as(i64, NOW_MS - MS_PER_SECOND), after.affinity);
}

test "renew clamps the new deadline to the hard cap when TTL would overshoot it" {
    var h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    teardown(conn);
    try base.seedTenant(conn); // workspaces.tenant_id FK requires the tenant first
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    // created_at is set so the cap lands BETWEEN now and now+TTL: the renewal is
    // legal (cap > now) but clamps to the cap, not the full TTL increment.
    const cap_offset: i64 = constants.LEASE_TTL_MS - 5_000; // < LEASE_TTL_MS
    const created = NOW_MS + cap_offset - constants.MAX_RUNTIME_MS;
    try seedAffinity(conn, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(conn, 5, created, NOW_MS - MS_PER_SECOND);
    defer teardown(conn);

    const outcome = try renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, .{});
    const want_cap = NOW_MS + cap_offset; // == created + MAX_RUNTIME_MS
    try std.testing.expectEqual(renewal.RenewOutcome{ .renewed = want_cap }, outcome);
    const after = try readDeadlines(conn);
    try std.testing.expectEqual(want_cap, after.lease);
    try std.testing.expectEqual(want_cap, after.affinity);
}
