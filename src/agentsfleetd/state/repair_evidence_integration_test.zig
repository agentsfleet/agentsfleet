//! Live PostgreSQL proof that one repair correlation crosses its keyset page.

const std = @import("std");
const pg = @import("pg");

const base = @import("../db/test_fixtures.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const repair_evidence = @import("repair_evidence.zig");
const repair_results = @import("repair_production_results.zig");

const testing = std.testing;
const ALLOC = testing.allocator;

const VERIFIER_COUNT: i32 = 101;
const NOW_MS: i64 = 1_800_100_000_000;
const OBSERVATION_WINDOW_MS: i64 = 15 * std.time.ms_per_min;
const TENANT_ID = "0195c102-8000-7000-8000-000000000001";
const WORKSPACE_ID = "0195c102-8001-7000-8000-000000000001";
const INCIDENT_FLEET_ID = "0195c102-8002-7000-8000-000000000001";
const REPAIR_LINK_ID = "0195c102-8003-7000-8000-000000000001";
const INCIDENT_EVENT_ID = "repair-fanout-incident";
const REPOSITORY = "agentsfleet/agentsfleet";
const MERGED_COMMIT_SHA = "repair-fanout-merged-commit";
const PROVIDER_STATUS_ID = "repair-fanout-status";
const VERIFIER_CONFIG =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["repair_production_result"],"repositories":["agentsfleet/agentsfleet"]}],"repositories":["agentsfleet/agentsfleet"],"repository_access":"read"}}
;

fn resetFixture(conn: *pg.Conn) !void {
    _ = try conn.exec("BEGIN", .{});
    errdefer _ = conn.exec("ROLLBACK", .{}) catch null;
    _ = try conn.exec("SET LOCAL fleet.allow_gate_purge = 'on'", .{});
    _ = try conn.exec("DELETE FROM core.workspaces WHERE id = $1::uuid", .{WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM core.tenants WHERE id = $1::uuid", .{TENANT_ID});
    _ = try conn.exec("COMMIT", .{});
}

fn resetFixtureBestEffort(conn: *pg.Conn) void {
    resetFixture(conn) catch |err| std.log.warn("repair fanout fixture cleanup ignored: {s}", .{@errorName(err)});
}

fn seedFixture(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_ID, "repair-fanout-suite");
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'repair-fanout-incident',
        \\        '# incident', '{}'::jsonb, 'active', $4, $4)
    , .{ INCIDENT_FLEET_ID, WORKSPACE_ID, TENANT_ID, NOW_MS });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
        \\   request_json, response_text, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'test:incident', 'webhook', 'processed',
        \\        '{"symptom":"latency"}'::jsonb, 'Latency followed the deployment.', $4, $4)
    , .{ INCIDENT_FLEET_ID, WORKSPACE_ID, INCIDENT_EVENT_ID, NOW_MS });
    _ = try conn.exec(
        \\INSERT INTO core.repair_pr_links
        \\  (id, workspace_id, fleet_id, event_id, repository, branch,
        \\   pr_number, pr_url, deploy_status, merged_commit_sha, merged_at, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5,
        \\        'agentsfleet-repair/fanout', 157,
        \\        'https://github.com/agentsfleet/agentsfleet/pull/157',
        \\        'pending', $6, $7, $7)
    , .{ REPAIR_LINK_ID, WORKSPACE_ID, INCIDENT_FLEET_ID, INCIDENT_EVENT_ID, REPOSITORY, MERGED_COMMIT_SHA, NOW_MS });
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json,
        \\   status, created_at, updated_at)
        \\SELECT
        \\  ('0195c102-82' || lpad(to_hex(g), 2, '0') ||
        \\   '-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
        \\  $1::uuid, $2::uuid, 'repair-fanout-verifier-' || g::text,
        \\  '# verifier', $3::jsonb, 'active', $4, $4
        \\FROM generate_series(1, $5::int) AS g
    , .{ WORKSPACE_ID, TENANT_ID, VERIFIER_CONFIG, NOW_MS, VERIFIER_COUNT });
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (id, fleet_id, service, status, requested_reason, approved_at, created_at)
        \\SELECT
        \\  ('0195c102-83' || lpad(to_hex(g), 2, '0') ||
        \\   '-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
        \\  ('0195c102-82' || lpad(to_hex(g), 2, '0') ||
        \\   '-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
        \\  'github', 'approved', 'repair fanout proof', $1, $1
        \\FROM generate_series(1, $2::int) AS g
    , .{ NOW_MS, VERIFIER_COUNT });
}

const VerificationCounts = struct {
    total: i64,
    fleets: i64,
    earliest: i64,
    latest: i64,
};

fn verificationCounts(conn: *pg.Conn) !VerificationCounts {
    var query = PgQuery.from(try conn.query(
        \\SELECT count(*), count(DISTINCT verifier_fleet_id),
        \\       min(verify_after), max(verify_after)
        \\FROM core.repair_verifications
        \\WHERE repair_link_id = $1::uuid
    , .{REPAIR_LINK_ID}));
    defer query.deinit();
    const row = try query.next() orelse return error.TestUnexpectedResult;
    const counts = VerificationCounts{
        .total = try row.get(i64, 0),
        .fleets = try row.get(i64, 1),
        .earliest = try row.get(i64, 2),
        .latest = try row.get(i64, 3),
    };
    query.drain();
    return counts;
}

fn expectLedgersRejectMutation(conn: *pg.Conn) !void {
    try testing.expectError(error.PG, conn.exec(
        "UPDATE core.repair_production_results SET conclusion = 'failure' WHERE provider_status_id = $1",
        .{PROVIDER_STATUS_ID},
    ));
    try testing.expectError(error.PG, conn.exec(
        "DELETE FROM core.repair_production_results WHERE provider_status_id = $1",
        .{PROVIDER_STATUS_ID},
    ));
    try testing.expectError(error.PG, conn.exec(
        "UPDATE core.repair_verifications SET verify_after = verify_after + 1 WHERE repair_link_id = $1::uuid",
        .{REPAIR_LINK_ID},
    ));
    try testing.expectError(error.PG, conn.exec(
        "DELETE FROM core.repair_verifications WHERE repair_link_id = $1::uuid",
        .{REPAIR_LINK_ID},
    ));
}

test "integration: one repair correlation crosses the one hundred row keyset page" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try resetFixture(db.conn);
    defer resetFixtureBestEffort(db.conn);
    try seedFixture(db.conn);

    const result = repair_results.NewResult{
        .workspace_id = WORKSPACE_ID,
        .provider = repair_evidence.GITHUB_PROVIDER,
        .provider_deployment_id = "repair-fanout-deployment",
        .provider_status_id = PROVIDER_STATUS_ID,
        .repository = REPOSITORY,
        .environment = repair_evidence.PRODUCTION_ENVIRONMENT,
        .commit_sha = MERGED_COMMIT_SHA,
        .conclusion = repair_evidence.SUCCESS_CONCLUSION,
        .completed_at = NOW_MS,
    };
    const arrival = try repair_evidence.recordProduction(ALLOC, db.conn, result);
    try testing.expectEqual(.inserted, arrival.outcome);
    try testing.expectEqual(@as(usize, @intCast(VERIFIER_COUNT)), arrival.verification_attempts);
    const counts = try verificationCounts(db.conn);
    try testing.expectEqual(@as(i64, VERIFIER_COUNT), counts.total);
    try testing.expectEqual(@as(i64, VERIFIER_COUNT), counts.fleets);
    try testing.expectEqual(NOW_MS + OBSERVATION_WINDOW_MS, counts.earliest);
    try testing.expectEqual(counts.earliest, counts.latest);

    const replay = try repair_evidence.recordProduction(ALLOC, db.conn, result);
    try testing.expectEqual(.replayed, replay.outcome);
    try testing.expectEqual(@as(usize, 0), replay.verification_attempts);
    try testing.expectEqual(@as(i64, VERIFIER_COUNT), (try verificationCounts(db.conn)).total);
    try expectLedgersRejectMutation(db.conn);
}
