//! Allocation-failure proofs for the event-detail read and the grant set read.
//!
//! Both build owned memory behind `errdefer` rungs that no successful call
//! touches, and both were left unproven for the same reason: the existing
//! fixtures never produce the shape the rungs need.
//!
//! `readRow` guards five OPTIONAL dupes — response_text, failure_label,
//! failure_detail, checkpoint_id, resumes_event_id. An optional rung is the
//! easiest kind to get wrong, because a null column makes the free look
//! unnecessary right up until the row that has the field. The existing
//! thread-read proof seeds a response body and nothing else, so four of the
//! five have never run. The fixture here populates ALL five, which is what
//! turns those rungs into live allocation sites rather than skipped branches.
//!
//! `approvedSet` guards a growing list plus the dupe about to be appended to
//! it. Reaching the inner rung needs more than one grant row: with a single
//! service the list never holds anything when the append fails.

const std = @import("std");
const pg = @import("pg");

const detail_store = @import("fleet_event_detail_store.zig");
const events_store = @import("fleet_events_store.zig");
const grant_lookup = @import("integration_grant_lookup.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

const TENANT_ID = "0196a100-0000-7000-8000-00000000e001";
const WS_ID = "0196a100-0000-7000-8000-00000000e002";
const FLEET_ID = "0196a100-0000-7000-8000-00000000e003";

const EVENT_ID = "evt-detail-alloc-01";
const RESUMED_EVENT_ID = "evt-detail-alloc-00";
const EVENT_ID_2 = "evt-detail-alloc-02";

const GRANT_A = "0196a100-0000-7000-8000-00000000e011";
const GRANT_B = "0196a100-0000-7000-8000-00000000e012";
const GRANT_C = "0196a100-0000-7000-8000-00000000e013";

const CREATED_MS: i64 = 1_770_000_000_000;
/// Gap between the two seeded events — any non-zero value orders them; the
/// list read is keyset-ordered on (created_at, event_id), so they must differ.
const SECOND_EVENT_OFFSET_MS: i64 = 1_000;

/// Every optional column carries a value. A null in any of them silently
/// removes one of the five rungs this file exists to exercise.
fn seed(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_ID, "fleet-detail-alloc");
    try base.seedWorkspaceWithTenant(conn, WS_ID, TENANT_ID);
    try base.seedFleet(conn, FLEET_ID, WS_ID, "detail-alloc", "{}", "# detail");

    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, response_text, tokens, wall_ms,
        \\   failure_label, failure_detail, checkpoint_id, resumes_event_id,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, 'steer:alloc', 'chat', 'failed',
        \\        $4::jsonb, $5, 128, 4200,
        \\        'runner_crash', 'the runner exited before it answered',
        \\        'ckpt-detail-alloc-01', $6,
        \\        $7, $7)
    , .{
        FLEET_ID,
        EVENT_ID,
        WS_ID,
        "{\"message\":\"detail alloc fixture\"}",
        "the answer this run produced",
        RESUMED_EVENT_ID,
        CREATED_MS,
    });
    // A second row with the same full shape. The per-fleet LIST read guards a
    // partial page, and a one-row fixture can never fail an append with rows
    // already in the list.
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, response_text, tokens, wall_ms,
        \\   failure_label, failure_detail, checkpoint_id, resumes_event_id,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, 'steer:alloc', 'chat', 'failed',
        \\        $4::jsonb, $5, 256, 8400,
        \\        'runner_timeout', 'the runner passed its deadline',
        \\        'ckpt-detail-alloc-02', $6,
        \\        $7, $7)
    , .{
        FLEET_ID,
        EVENT_ID_2,
        WS_ID,
        "{\"message\":\"detail alloc fixture two\"}",
        "the answer the second run produced",
        EVENT_ID,
        CREATED_MS + SECOND_EVENT_OFFSET_MS,
    });

    // Three services, so the inner rung fails with a list that already owns
    // entries rather than an empty one.
    inline for (.{ .{ GRANT_A, "github" }, .{ GRANT_B, "slack" }, .{ GRANT_C, "linear" } }) |g| {
        _ = try conn.exec(
            \\INSERT INTO core.integration_grants
            \\  (id, fleet_id, service, status, requested_reason, approved_at, created_at)
            \\VALUES ($1::uuid, $2::uuid, $3, 'approved', 'alloc proof', $4, $4)
        , .{ g[0], FLEET_ID, g[1], CREATED_MS });
    }
}

fn teardown(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.integration_grants WHERE fleet_id = $1::uuid", .{FLEET_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleet_events WHERE workspace_id = $1::uuid", .{WS_ID}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    base.teardownFleets(conn, WS_ID);
    base.teardownWorkspace(conn, WS_ID);
    base.teardownTenantById(conn, TENANT_ID);
}

// ── Wrappers ──────────────────────────────────────────────────────────────

fn getForFleetUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var row = (try detail_store.getForFleet(conn, alloc, WS_ID, FLEET_ID, EVENT_ID)) orelse
        return error.FixtureEventMissing;
    row.deinit(alloc);
}

/// The per-fleet list read. Lives here rather than beside the filter tests
/// because that suite's events carry NULL in every failure column, so four of
/// the five optional rungs are not allocation sites there at all — a proof
/// written against it passes with those rungs deleted.
fn listForFleetUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    const rows = try events_store.listForFleet(conn, alloc, WS_ID, FLEET_ID, .{ .limit = 50 });
    for (rows) |*r| r.deinit(alloc);
    alloc.free(rows);
}

fn approvedSetUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    const set = try grant_lookup.approvedSet(alloc, conn, FLEET_ID);
    for (set) |s| alloc.free(s);
    alloc.free(set);
}

// ── Proofs ────────────────────────────────────────────────────────────────

test "integration: every allocation site in the event detail read unwinds without leaking" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    teardown(db_ctx.conn);
    try seed(db_ctx.conn);
    defer teardown(db_ctx.conn);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        getForFleetUnderAllocator,
        .{db_ctx.conn},
    );
}

test "integration: every allocation site in the approved grant set unwinds without leaking" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    teardown(db_ctx.conn);
    try seed(db_ctx.conn);
    defer teardown(db_ctx.conn);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        approvedSetUnderAllocator,
        .{db_ctx.conn},
    );
}

test "integration: every allocation site in the per-fleet event list unwinds without leaking" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    teardown(db_ctx.conn);
    try seed(db_ctx.conn);
    defer teardown(db_ctx.conn);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        listForFleetUnderAllocator,
        .{db_ctx.conn},
    );
}
