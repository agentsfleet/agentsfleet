//! GET /v1/fleets/runners/{runner_id} — operator-plane single-runner read.
//!
//! The detail page's cold-load hydration source: the runner record with
//! derived liveness, a live-work summary, and lifetime counters computed from
//! durable lease and event rows — never from the per-runner Prometheus
//! families, which are process-local, zeroed on restart, and capped
//! (docs/architecture/runner_fleet.md "Observability").

const std = @import("std");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const sql = @import("sql.zig");
const runner_row = @import("runner_row.zig");
const protocol = @import("contract").protocol;
const constants = @import("common");
const logging = @import("log");

const log = logging.scoped(.fleet_runner_get);

const Hx = hx_mod.Hx;

/// Wire shape — the resource itself, no envelope. `token_hash` and the stored
/// auth status have no field here, so emitting either is a compile error.
const RunnerDetail = struct {
    id: []const u8,
    host_id: []const u8,
    sandbox_tier: []const u8,
    admin_state: protocol.AdminState,
    liveness: protocol.RunnerLiveness,
    labels: []const []const u8,
    last_seen_at: i64,
    created_at: i64,
    assigned_policy: ?protocol.AssignedPolicy,
    achievable: ?protocol.CapabilityReport,
    degraded: bool,
    degraded_reason: ?[]const u8,
    active_lease_count: i64,
    active_fleet_count: i64,
    leases_acquired: i64,
    leases_succeeded: i64,
    leases_failed: i64,
    leases_expired: i64,
    /// An operator's outstanding ask; null when none is pending. Non-null means
    /// asked-but-not-yet-answered — the daemon clears it on the beat that
    /// reports the matching verdict.
    selftest_requested_at: ?i64,
    /// When the stored verdict landed; null until a first report.
    selftest_completed_at: ?i64,
    /// The latest verdict, or null for a runner that has never self-tested —
    /// which the page renders differently from "tested and found no checks".
    selftest: ?protocol.SelftestReport,
};

pub fn innerGetFleetRunner(hx: Hx, runner_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, runner_id, "runner_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const now_ms = constants.clock.nowMillis();
    const detail = fetchDetail(conn, hx.alloc, runner_id, now_ms) catch |err| switch (err) {
        error.RunnerNotFound => {
            hx.fail(ec.ERR_RUNNER_NOT_FOUND, "Runner not found");
            return;
        },
        else => {
            log.err("runner_get_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };
    hx.ok(.ok, detail);
}

/// The busy check rides the live-lease count the summary already computed, so
/// the single read and the list read cannot disagree on what busy means: both
/// funnel through `runner_row.deriveLiveness`.
fn livenessFromCounts(last_seen_at: i64, active_lease_count: i64, now_ms: i64) protocol.RunnerLiveness {
    return runner_row.deriveLiveness(last_seen_at, active_lease_count > 0, now_ms);
}

fn fetchDetail(conn: anytype, alloc: std.mem.Allocator, runner_id: []const u8, now_ms: i64) !RunnerDetail {
    var q = PgQuery.from(try conn.query(sql.SELECT_RUNNER_DETAIL, .{
        runner_id,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        now_ms,
    }));
    defer q.deinit();

    const row = (try q.next()) orelse return error.RunnerNotFound;

    const raw_admin_state = try row.get([]u8, 3);
    const admin_state = std.meta.stringToEnum(protocol.AdminState, raw_admin_state) orelse return error.DbRowShape;
    const last_seen_at = try row.get(i64, 5);
    const created_at = try row.get(i64, 6);
    const active_lease_count = try row.get(i64, 7);
    const active_fleet_count = try row.get(i64, 8);
    const leases_acquired = try row.get(i64, 9);
    const leases_succeeded = try row.get(i64, 10);
    const leases_failed = try row.get(i64, 11);
    const leases_expired = try row.get(i64, 12);
    const tier_raw = try row.get([]u8, 2);
    // Columns 13–19: the shared M148 policy/verdict tail (same order as the
    // list statement), decoded by the same helper so the two reads agree.
    const policy = try runner_row.readPolicyColumns(alloc, row, tier_raw, 13);
    errdefer if (policy.degraded_reason) |r| alloc.free(r);

    // Columns 20–25: the self-test slot. The detail read carries it; the list
    // read does not, because a per-check verdict list is a page, not a cell.
    const selftest_requested_at = try row.get(?i64, 20);
    const selftest_completed_at = try row.get(?i64, 21);
    const selftest = runner_row.decodeSelftest(
        alloc,
        try row.get(?[]u8, 22),
        try row.get(?bool, 23),
        try row.get(?[]u8, 24),
        try row.get(?[]u8, 25),
    );

    const id = try alloc.dupe(u8, try row.get([]u8, 0));
    errdefer alloc.free(id);
    const host_id = try alloc.dupe(u8, try row.get([]u8, 1));
    errdefer alloc.free(host_id);
    const sandbox_tier = try alloc.dupe(u8, tier_raw);
    errdefer alloc.free(sandbox_tier);

    return .{
        .id = id,
        .host_id = host_id,
        .sandbox_tier = sandbox_tier,
        .admin_state = admin_state,
        .liveness = livenessFromCounts(last_seen_at, active_lease_count, now_ms),
        .labels = runner_row.parseLabels(alloc, try row.get([]u8, 4)),
        .last_seen_at = last_seen_at,
        .created_at = created_at,
        .assigned_policy = policy.assigned_policy,
        .achievable = policy.achievable,
        .degraded = policy.degraded,
        .degraded_reason = policy.degraded_reason,
        .active_lease_count = active_lease_count,
        .active_fleet_count = active_fleet_count,
        .leases_acquired = leases_acquired,
        .leases_succeeded = leases_succeeded,
        .leases_failed = leases_failed,
        .leases_expired = leases_expired,
        .selftest_requested_at = selftest_requested_at,
        .selftest_completed_at = selftest_completed_at,
        .selftest = selftest,
    };
}

const TEST_NOW_MS: i64 = 1_000_000;
const TEST_FRESH_SEEN_MS: i64 = TEST_NOW_MS - 1;
const TEST_STALE_SEEN_MS: i64 = TEST_NOW_MS - constants.RUNNER_OFFLINE_AFTER_MS - 1;

test "test_runner_get_liveness_agrees_with_list" {
    // Matrix of last_seen (never / fresh / stale) × live-lease count (0, 1, 3):
    // the single read's count-based mapping must agree with the list read's
    // boolean derivation for every cell, so the two surfaces cannot drift.
    const seen_cases = [_]i64{ protocol.RUNNER_LAST_SEEN_NEVER, TEST_FRESH_SEEN_MS, TEST_STALE_SEEN_MS };
    const lease_counts = [_]i64{ 0, 1, 3 };
    for (seen_cases) |last_seen| {
        for (lease_counts) |count| {
            try std.testing.expectEqual(
                runner_row.deriveLiveness(last_seen, count > 0, TEST_NOW_MS),
                livenessFromCounts(last_seen, count, TEST_NOW_MS),
            );
        }
    }
    // Boundary anchors: never-seen wins over a live lease being absent, a live
    // lease wins over staleness, and a stale idle runner reads offline.
    try std.testing.expectEqual(protocol.RunnerLiveness.registered, livenessFromCounts(protocol.RUNNER_LAST_SEEN_NEVER, 0, TEST_NOW_MS));
    try std.testing.expectEqual(protocol.RunnerLiveness.busy, livenessFromCounts(TEST_STALE_SEEN_MS, 2, TEST_NOW_MS));
    try std.testing.expectEqual(protocol.RunnerLiveness.online, livenessFromCounts(TEST_FRESH_SEEN_MS, 0, TEST_NOW_MS));
    try std.testing.expectEqual(protocol.RunnerLiveness.offline, livenessFromCounts(TEST_STALE_SEEN_MS, 0, TEST_NOW_MS));
}
