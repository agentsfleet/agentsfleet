// The allocation unwind under repair-verification dispatch, and the queued-at
// probe the Fleet report path takes.
//
// `claimDue` duplicates fourteen strings per claimed row, each behind its own
// errdefer, on top of a claim token and a row list carrying an unwind of its
// own. Nothing drove that ladder: the dispatcher suite only ever claims
// successfully, so every errdefer in `copyDue` read dark, and a gap anywhere in
// the chain would leak in production under memory pressure and nowhere else.
// The sweep below fails at each allocation index in turn, so
// `std.testing.allocator` reports the gap here instead.
//
// `verifierQueuedAt` was never called at all. It is the probe the Fleet report
// path takes to decide whether a report belongs to a verifier intent, so both
// of its answers — an enqueue time, and nothing — are asserted.
//
// Self-contained rows under an id prefix no sibling suite uses: the dispatcher
// suite purges its whole workspace on teardown, and a verification this suite
// is mid-sweep on is exactly the row that purge would erase.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const pg = @import("pg");

const base = @import("../db/test_fixtures.zig");
const repair_evidence = @import("repair_evidence.zig");
const store = @import("repair_verifications.zig");

const ALLOC = std.testing.allocator;

const TENANT_ID = "0196a200-0000-7000-8000-00000000a001";
const WS_ID = "0196a200-0000-7000-8000-00000000a002";
const INCIDENT_FLEET_ID = "0196a200-0000-7000-8000-00000000a003";
const VERIFIER_FLEET_ONE = "0196a200-0000-7000-8000-00000000a004";
const VERIFIER_FLEET_TWO = "0196a200-0000-7000-8000-00000000a005";
const REPAIR_LINK_ID = "0196a200-0000-7000-8000-00000000a006";
const PRODUCTION_RESULT_ID = "0196a200-0000-7000-8000-00000000a007";
const DUE_ONE = "0196a200-0000-7000-8000-00000000a008";
const DUE_TWO = "0196a200-0000-7000-8000-00000000a009";
const CLEARED_ONE = "0196a200-0000-7000-8000-00000000a00a";
const CLEARED_TWO = "0196a200-0000-7000-8000-00000000a00b";

const INCIDENT_EVENT_ID = "repair-unwind-incident";
const VERIFIER_EVENT_ONE = "repair-unwind-verifier-1";
const VERIFIER_EVENT_TWO = "repair-unwind-verifier-2";
const UNRELATED_EVENT_ID = "repair-unwind-unrelated";

const REPOSITORY = "agentsfleet/agentsfleet";
const MERGED_COMMIT_SHA = "repair-unwind-merged-commit";
const BRANCH = "agentsfleet-repair/repair-unwind-incident";
const PR_URL = "https://github.com/agentsfleet/agentsfleet/pull/164";
const DEPLOY_STATUS_PENDING = "pending";
const EVENT_STATUS_PROCESSED = "processed";
const EVENT_TYPE_CHAT = "chat";
const ACTOR_INCIDENT = "test:incident";
const REQUEST_JSON = "{\"symptom\":\"latency\"}";
const RESPONSE_TEXT = "Latency began after the deployment.";
const FLEET_CONFIG = "{}";
const FLEET_SOURCE = "# repair unwind fixture";

/// Opens the append-only tables to the cascade for the length of one
/// transaction. Without it the purge is refused and the fixture cannot reset.
const PURGE_GATE_SETTING = "fleet.allow_gate_purge = 'on'";

// Fixed rather than clock-derived: the claim window and the cleanup window are
// both assertions about ordering, and a per-run clock makes them a moving
// target.
const BASE_MS: i64 = 1_800_000_000_000;
const PR_NUMBER: i64 = 164;

// Each failed claim still commits its UPDATE, so the row carries a claim token
// and `dispatch_claimed_at` from the previous pass. Advancing the clock by more
// than the stale window per iteration re-opens the row through the
// stale-reclaim arm, which is what lets one seeded row serve the whole sweep.
const RECLAIM_STEP_MS: i64 = 2 * store.CLAIM_STALE_MS;

// One past the allocation count of a two-row claim: a claim token, fourteen
// duplicated strings per row, the row list's growth, and the owned slice.
const CLAIM_ALLOCATION_CEILING: usize = 48;
// A cleanup page duplicates one id per row and owns the slice.
const CLEANUP_ALLOCATION_CEILING: usize = 12;

const CLEANUP_ROW_COUNT: usize = 2;

fn seedFleetEvent(
    conn: *pg.Conn,
    fleet_id: []const u8,
    event_id: []const u8,
    created_at: i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
        \\   request_json, response_text, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7::jsonb, $8, $9, $9)
    , .{
        fleet_id,
        WS_ID,
        event_id,
        ACTOR_INCIDENT,
        EVENT_TYPE_CHAT,
        EVENT_STATUS_PROCESSED,
        REQUEST_JSON,
        RESPONSE_TEXT,
        created_at,
    });
}

/// A verification the dispatcher has already completed, left with its Redis
/// once-key uncleared so the cleanup page picks it up.
fn seedClearedVerification(
    conn: *pg.Conn,
    id: []const u8,
    verifier_fleet_id: []const u8,
    verifier_event_id: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.repair_verifications
        \\  (id, workspace_id, production_result_id, repair_link_id,
        \\   verifier_fleet_id, verifier_event_id, verify_after,
        \\   dispatch_attempts, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, $7, 1, $7, $7)
    , .{ id, WS_ID, PRODUCTION_RESULT_ID, REPAIR_LINK_ID, verifier_fleet_id, verifier_event_id, BASE_MS });
}

fn seed(conn: *pg.Conn) !void {
    // A run killed before its teardown leaves append-only rows that no
    // ON CONFLICT clause can absorb, so every seed starts from a purge.
    teardown(conn);
    try base.seedTenantById(conn, TENANT_ID, "repair-unwind-suite");
    try base.seedWorkspaceWithTenant(conn, WS_ID, TENANT_ID);
    try base.seedFleet(conn, INCIDENT_FLEET_ID, WS_ID, "repair-unwind-incident", FLEET_CONFIG, FLEET_SOURCE);
    try base.seedFleet(conn, VERIFIER_FLEET_ONE, WS_ID, "repair-unwind-verifier-1", FLEET_CONFIG, FLEET_SOURCE);
    try base.seedFleet(conn, VERIFIER_FLEET_TWO, WS_ID, "repair-unwind-verifier-2", FLEET_CONFIG, FLEET_SOURCE);

    try seedFleetEvent(conn, INCIDENT_FLEET_ID, INCIDENT_EVENT_ID, BASE_MS - 4);
    try seedFleetEvent(conn, INCIDENT_FLEET_ID, UNRELATED_EVENT_ID, BASE_MS - 4);
    try seedFleetEvent(conn, VERIFIER_FLEET_ONE, VERIFIER_EVENT_ONE, BASE_MS - 1);
    try seedFleetEvent(conn, VERIFIER_FLEET_TWO, VERIFIER_EVENT_TWO, BASE_MS - 1);

    try seedMergedEvidence(conn);
}

/// The merged pull request and its production result — the two rows `claimDue`
/// joins through, and the source of every string `copyDue` duplicates.
fn seedMergedEvidence(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.repair_pr_links
        \\  (id, workspace_id, fleet_id, event_id, repository, branch,
        \\   pr_number, pr_url, deploy_status, created_at,
        \\   merged_commit_sha, merged_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $12)
    , .{
        REPAIR_LINK_ID,
        WS_ID,
        INCIDENT_FLEET_ID,
        INCIDENT_EVENT_ID,
        REPOSITORY,
        BRANCH,
        PR_NUMBER,
        PR_URL,
        DEPLOY_STATUS_PENDING,
        BASE_MS - 3,
        MERGED_COMMIT_SHA,
        BASE_MS - 2,
    });
    _ = try conn.exec(
        \\INSERT INTO core.repair_production_results
        \\  (id, workspace_id, provider, provider_deployment_id,
        \\   provider_status_id, repository, environment, commit_sha,
        \\   conclusion, completed_at, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $10)
    , .{
        PRODUCTION_RESULT_ID,
        WS_ID,
        repair_evidence.GITHUB_PROVIDER,
        "repair-unwind-deployment",
        "repair-unwind-status",
        REPOSITORY,
        repair_evidence.PRODUCTION_ENVIRONMENT,
        MERGED_COMMIT_SHA,
        repair_evidence.SUCCESS_CONCLUSION,
        BASE_MS - 1,
    });
}

/// Two rows, so a failure part-way through the second walks the row list's own
/// unwind over the first — the arm a single-row page cannot reach.
fn seedDue(conn: *pg.Conn) !void {
    for ([_][]const u8{ DUE_ONE, DUE_TWO }, [_][]const u8{ VERIFIER_FLEET_ONE, VERIFIER_FLEET_TWO }) |id, fleet_id| {
        _ = try conn.exec(
            \\INSERT INTO core.repair_verifications
            \\  (id, workspace_id, production_result_id, repair_link_id,
            \\   verifier_fleet_id, verify_after, dispatch_attempts,
            \\   created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 0, $6, $6)
        , .{ id, WS_ID, PRODUCTION_RESULT_ID, REPAIR_LINK_ID, fleet_id, BASE_MS });
    }
}

/// `core.repair_pr_links`, `core.repair_production_results` and
/// `core.repair_verifications` are append-only at the database level: the first
/// two refuse a DELETE outright and the third permits only the fenced claim,
/// completion and cleanup writes. Purging the workspace behind the gate setting
/// and letting the cascade reach them is the only way this fixture can be torn
/// down, which is why the dispatcher suite resets the same way.
fn purgeFixture(conn: *pg.Conn) !void {
    _ = try conn.exec("BEGIN", .{});
    errdefer _ = conn.exec("ROLLBACK", .{}) catch null;
    _ = try conn.exec("SET LOCAL " ++ PURGE_GATE_SETTING, .{});
    _ = try conn.exec("DELETE FROM core.workspaces WHERE id = $1::uuid", .{WS_ID});
    _ = try conn.exec("DELETE FROM core.tenants WHERE id = $1::uuid", .{TENANT_ID});
    _ = try conn.exec("COMMIT", .{});
}

/// Reports the server's message rather than the bare `error.PG`: a teardown that
/// swallows the reason is how the next test's duplicate-key failure gets blamed
/// on the wrong statement.
fn teardown(conn: *pg.Conn) void {
    purgeFixture(conn) catch |err| {
        const detail = if (conn.err) |pg_err| pg_err.message else @errorName(err);
        std.log.warn("teardown ignored: {s}", .{detail});
    };
}

test "integration: a repair-verification claim that runs out of memory mid-row frees every string it took" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn);
    try seedDue(db_ctx.conn);
    defer teardown(db_ctx.conn);

    var fail_index: usize = 0;
    var saw_oom = false;
    while (fail_index < CLAIM_ALLOCATION_CEILING) : (fail_index += 1) {
        var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = fail_index });
        const now_ms = BASE_MS + @as(i64, @intCast(fail_index)) * RECLAIM_STEP_MS;
        const result = store.claimDue(failing.allocator(), db_ctx.conn, now_ms);
        if (result) |claimed| {
            // Past the last allocation the claim succeeds; free through the same
            // allocator that served it and stop.
            var owned = claimed;
            owned.deinit(failing.allocator());
            break;
        } else |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
            saw_oom = true;
        }
    }
    try std.testing.expect(saw_oom);
    // The sweep must have reached a successful claim, or the ceiling is too low
    // and the tail of the ladder is still unproven.
    try std.testing.expect(fail_index < CLAIM_ALLOCATION_CEILING);
}

test "integration: a repair-verification cleanup page that runs out of memory frees the ids it took" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn);
    try seedClearedVerification(db_ctx.conn, CLEARED_ONE, VERIFIER_FLEET_ONE, VERIFIER_EVENT_ONE);
    try seedClearedVerification(db_ctx.conn, CLEARED_TWO, VERIFIER_FLEET_TWO, VERIFIER_EVENT_TWO);
    defer teardown(db_ctx.conn);

    const now_ms = BASE_MS + RECLAIM_STEP_MS;

    var fail_index: usize = 0;
    var saw_oom = false;
    while (fail_index < CLEANUP_ALLOCATION_CEILING) : (fail_index += 1) {
        var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = fail_index });
        const result = store.redisCleanupDue(failing.allocator(), db_ctx.conn, now_ms);
        if (result) |rows| {
            try std.testing.expectEqual(CLEANUP_ROW_COUNT, rows.len);
            store.freeRedisCleanup(failing.allocator(), rows);
            break;
        } else |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
            saw_oom = true;
        }
    }
    try std.testing.expect(saw_oom);
    try std.testing.expect(fail_index < CLEANUP_ALLOCATION_CEILING);
}

test "integration: the repair-verification queued-at probe answers for a verifier report and stays silent for any other" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn);
    try seedClearedVerification(db_ctx.conn, CLEARED_ONE, VERIFIER_FLEET_ONE, VERIFIER_EVENT_ONE);
    defer teardown(db_ctx.conn);

    // The verifier's own report reads the enqueue time off the Fleet event the
    // dispatcher created, which is what the report path needs to measure the
    // verification's latency.
    const queued_at = try store.verifierQueuedAt(db_ctx.conn, VERIFIER_FLEET_ONE, VERIFIER_EVENT_ONE);
    try std.testing.expectEqual(@as(?i64, BASE_MS - 1), queued_at);

    // An ordinary Fleet report on a fleet with no verification intent is
    // deliberately invisible here — the report path must not treat it as a
    // verification just because the event exists.
    try std.testing.expectEqual(
        @as(?i64, null),
        try store.verifierQueuedAt(db_ctx.conn, INCIDENT_FLEET_ID, UNRELATED_EVENT_ID),
    );

    // A verifier fleet carrying no completed verification answers nothing
    // either: the probe keys on the verification row, not on the fleet.
    try std.testing.expectEqual(
        @as(?i64, null),
        try store.verifierQueuedAt(db_ctx.conn, VERIFIER_FLEET_TWO, VERIFIER_EVENT_TWO),
    );
}
