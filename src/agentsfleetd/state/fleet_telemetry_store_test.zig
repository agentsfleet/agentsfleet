// Integration tests for fleet_telemetry_store.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const store = @import("fleet_telemetry_store.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");
const MS_PER_SECOND = 1000;

const ALLOC = std.testing.allocator;
const WS_A = "0195b4ba-8d3a-7f13-8abc-aa1900000001";
// A real `core.fleets` id, not a label. §3 moved the ledger's `fleet_id` from
// bare TEXT to `UUID REFERENCES core.fleets(id)`, so the old `"fleet-telem-a"`
// is refused by the driver before it reaches Postgres, and a UUID that names no
// row is refused by the foreign key. The third group must start with `7` to
// satisfy `ck_fleets_id_uuidv7`.
const AGENTSFLEET_A = "0195b4ba-8d3a-7f13-8abc-aa1900000101";
// Arbitrary model id for the telemetry fixtures — the value is opaque to these
// tests (they only store + read it back), so it's a plain test string.
const FIXTURE_MODEL = "telemetry-test-model";

fn seedStageRow(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8, recorded_at: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .event_id = event_id,
        .charge_type = .stage,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = 2,
        .event_created_at = recorded_at,
        .created_at = recorded_at,
    });
}

fn seedReceiveRow(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8, recorded_at: i64) !void {
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = base.TEST_TENANT_ID,
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .event_id = event_id,
        .charge_type = .receive,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = 1,
        .event_created_at = recorded_at,
        .created_at = recorded_at,
    });
}

fn teardownTelemetry(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM billing.usage_ledger WHERE workspace_id = $1", .{workspace_id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

// ── Read fixture ────────────────────────────────────────────────────────────
//
// The charges query is tenant-scoped, not workspace-scoped, so the read tests
// cannot share `base.TEST_TENANT_ID` with the insert tests above: a sibling
// suite's row under the same tenant would land in the middle of an asserted
// page. These own a tenant nothing else writes to.

const TENANT_READ = "0195b4ba-8d3a-7f13-8abc-aa1900000201";
const WS_READ = "0195b4ba-8d3a-7f13-8abc-aa1900000202";
const AGENTSFLEET_READ = "0195b4ba-8d3a-7f13-8abc-aa1900000203";

const READ_BASE_MS: i64 = 1_760_000_000_000;
const READ_FIRST_MS: i64 = READ_BASE_MS + MS_PER_SECOND;
const READ_SECOND_MS: i64 = READ_BASE_MS + 2 * MS_PER_SECOND;
const READ_THIRD_MS: i64 = READ_BASE_MS + 3 * MS_PER_SECOND;

const EVT_READ_FIRST = "evt-read-aa19-0001";
const EVT_READ_SECOND = "evt-read-aa19-0002";
const EVT_READ_THIRD = "evt-read-aa19-0003";

const READ_PAGE_LIMIT: u32 = 2;
// One past the allocation count of a two-row page: eight duplicated strings per
// row, the row list's growth, and the owned slice.
const READ_ALLOCATION_CEILING: usize = 30;

fn seedReadFixture(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_READ, "telemetry-read");
    try base.seedWorkspaceWithTenant(conn, WS_READ, TENANT_READ);
    try base.seedFleet(conn, AGENTSFLEET_READ, WS_READ, "telemetry read fixture", "{}", "");
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = TENANT_READ,
        .workspace_id = WS_READ,
        .fleet_id = AGENTSFLEET_READ,
        .event_id = EVT_READ_FIRST,
        .charge_type = .stage,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = 2,
        .event_created_at = READ_FIRST_MS,
        .created_at = READ_FIRST_MS,
    });
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = TENANT_READ,
        .workspace_id = WS_READ,
        .fleet_id = AGENTSFLEET_READ,
        .event_id = EVT_READ_SECOND,
        .charge_type = .receive,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = 1,
        .event_created_at = READ_SECOND_MS,
        .created_at = READ_SECOND_MS,
    });
    try store.insertTelemetry(conn, ALLOC, .{
        .tenant_id = TENANT_READ,
        .workspace_id = WS_READ,
        .fleet_id = AGENTSFLEET_READ,
        .event_id = EVT_READ_THIRD,
        .charge_type = .stage,
        .posture = .platform,
        .model = FIXTURE_MODEL,
        .credit_deducted_nanos = 3,
        .event_created_at = READ_THIRD_MS,
        .created_at = READ_THIRD_MS,
    });
}

fn teardownReadFixture(conn: *pg.Conn) void {
    teardownTelemetry(conn, WS_READ);
    base.teardownFleets(conn, WS_READ);
    base.teardownWorkspace(conn, WS_READ);
    base.teardownTenantById(conn, TENANT_READ);
}

fn freeRows(rows: []store.TelemetryRow) void {
    for (rows) |*r| r.deinit(ALLOC);
    ALLOC.free(rows);
}

// ── Read: the tenant charges page and the cursor that resumes it ────────────

test "list_telemetry_for_tenant_pages_newest_first_and_resumes_from_its_cursor" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    teardownReadFixture(db_ctx.conn);
    try seedReadFixture(db_ctx.conn);
    defer teardownReadFixture(db_ctx.conn);

    // Newest-first, bounded by the limit — the dashboard's Usage tab reads this
    // page directly, so the order is the contract, not an implementation detail.
    const first = try store.listTelemetryForTenant(db_ctx.conn, ALLOC, TENANT_READ, READ_PAGE_LIMIT, null);
    defer freeRows(first);
    try std.testing.expectEqual(@as(usize, READ_PAGE_LIMIT), first.len);
    try std.testing.expectEqualStrings(EVT_READ_THIRD, first[0].event_id);
    try std.testing.expectEqualStrings(EVT_READ_SECOND, first[1].event_id);

    // The cursor is built from the last row of the page it continues, and the
    // next page must start strictly after it — a boundary that repeats a row
    // would double-bill the reader's own total.
    const cursor = try store.makeCursor(ALLOC, first[first.len - 1]);
    defer ALLOC.free(cursor);
    const next = try store.listTelemetryForTenant(db_ctx.conn, ALLOC, TENANT_READ, READ_PAGE_LIMIT, cursor);
    defer freeRows(next);
    try std.testing.expectEqual(@as(usize, 1), next.len);
    try std.testing.expectEqualStrings(EVT_READ_FIRST, next[0].event_id);

    // Both nullable columns are populated here, so the row's optional frees are
    // taken rather than skipped.
    try std.testing.expect(next[0].workspace_id != null);
    try std.testing.expect(next[0].fleet_id != null);
}

test "list_telemetry_for_tenant_frees_every_string_when_a_page_runs_out_of_memory" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    teardownReadFixture(db_ctx.conn);
    try seedReadFixture(db_ctx.conn);
    defer teardownReadFixture(db_ctx.conn);

    // Each row duplicates eight strings, every one behind its own errdefer, over
    // a row list with an unwind of its own. Failing at each allocation index in
    // turn walks the whole ladder; a gap anywhere leaks, and the leak detector
    // reports it here rather than letting it drip in production under pressure.
    var fail_index: usize = 0;
    var saw_oom = false;
    while (fail_index < READ_ALLOCATION_CEILING) : (fail_index += 1) {
        var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = fail_index });
        const result = store.listTelemetryForTenant(
            db_ctx.conn,
            failing.allocator(),
            TENANT_READ,
            READ_PAGE_LIMIT,
            null,
        );
        if (result) |rows| {
            for (rows) |*r| r.deinit(failing.allocator());
            failing.allocator().free(rows);
            break;
        } else |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
            saw_oom = true;
        }
    }
    try std.testing.expect(saw_oom);
    // A sweep that never reaches a successful page proves only the early arms.
    try std.testing.expect(fail_index < READ_ALLOCATION_CEILING);
}

// ── Insert: idempotent on (event_id, charge_type) ───────────────────────────

test "insert_telemetry_idempotent_on_event_charge" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    // uc1 seeds the tenant and workspace only; the ledger's fleet foreign key
    // needs the fleet row to exist before any telemetry references it.
    try base.seedFleet(db_ctx.conn, AGENTSFLEET_A, WS_A, "telemetry fixture", "{}", "");
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-idem-aa19-0001";
    try seedStageRow(db_ctx.conn, WS_A, AGENTSFLEET_A, evt, MS_PER_SECOND);
    try seedStageRow(db_ctx.conn, WS_A, AGENTSFLEET_A, evt, MS_PER_SECOND); // duplicate — no-op

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM billing.usage_ledger WHERE workspace_id = $1 AND event_id = $2",
        .{ WS_A, evt },
    ));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

// ── Insert: receive + stage rows coexist for the same event_id ──────────────

test "insert_telemetry_two_rows_per_event" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    // uc1 seeds the tenant and workspace only; the ledger's fleet foreign key
    // needs the fleet row to exist before any telemetry references it.
    try base.seedFleet(db_ctx.conn, AGENTSFLEET_A, WS_A, "telemetry fixture", "{}", "");
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-two-aa19-0001";
    try seedReceiveRow(db_ctx.conn, WS_A, AGENTSFLEET_A, evt, MS_PER_SECOND);
    try seedStageRow(db_ctx.conn, WS_A, AGENTSFLEET_A, evt, 2000);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM billing.usage_ledger WHERE event_id = $1",
        .{evt},
    ));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 2), try row.get(i64, 0));
}

// ── Insert: receive row has NULL token counts ───────────────────────────────

test "insert_receive_has_null_tokens_and_wall_ms" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_A);
    // uc1 seeds the tenant and workspace only; the ledger's fleet foreign key
    // needs the fleet row to exist before any telemetry references it.
    try base.seedFleet(db_ctx.conn, AGENTSFLEET_A, WS_A, "telemetry fixture", "{}", "");
    defer uc1.teardown(db_ctx.conn, WS_A);
    defer teardownTelemetry(db_ctx.conn, WS_A);

    const evt = "evt-rcv-aa19-0001";
    try seedReceiveRow(db_ctx.conn, WS_A, AGENTSFLEET_A, evt, MS_PER_SECOND);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT token_count_input, token_count_output, wall_ms
        \\FROM billing.usage_ledger
        \\WHERE event_id = $1 AND charge_type = 'receive'
    , .{evt}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(?i64, null), try row.get(?i64, 0));
    try std.testing.expectEqual(@as(?i64, null), try row.get(?i64, 1));
    try std.testing.expectEqual(@as(?i64, null), try row.get(?i64, 2));
}
