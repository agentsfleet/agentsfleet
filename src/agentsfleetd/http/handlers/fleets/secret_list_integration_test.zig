//! Integration tier for the workspace credential list: its cost must not track
//! the number of stored credentials.
//!
//! Proven by counting REAL table access, not by reading the source. Postgres
//! tracks per-table scans in `pg_stat_all_tables`, so seeding one credential and
//! seeding many and comparing the scan delta answers "how many times did this
//! read `vault.secrets`" directly. `pg_stat_force_next_flush` makes the counters
//! readable in the same transaction rather than whenever the collector wakes.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("../../../db/test_fixtures.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const vault = @import("../../../state/vault.zig");
const cp = @import("../../../secrets/crypto_primitives.zig");
const secret_list = @import("secret_list.zig");

const WS_ONE = "0195b4ba-8d3a-7f13-8abc-0000000d0001";
const WS_MANY = "0195b4ba-8d3a-7f13-8abc-0000000d0002";

/// Credentials seeded into the "many" workspace. Twenty against one is a wide
/// enough spread that a per-credential read could not hide in the noise.
const MANY_SECRETS: usize = 20;

const TestDb = struct {
    pool: *pg.Pool,
    conn: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        return .{ .pool = ctx.pool, .conn = ctx.conn };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.conn);
        self.pool.deinit();
    }
};

/// Total scans recorded against `vault.secrets`, sequential plus index.
fn secretScans(conn: *pg.Conn) !i64 {
    _ = try conn.exec("SELECT pg_stat_force_next_flush()", .{});
    var q = PgQuery.from(try conn.query(
        \\SELECT COALESCE(seq_scan, 0) + COALESCE(idx_scan, 0)
        \\FROM pg_stat_all_tables
        \\WHERE schemaname = 'vault' AND relname = 'secrets'
    , .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return row.get(i64, 0);
}

fn seedSecrets(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, count: usize) !void {
    try base.seedWorkspace(conn, workspace_id);
    for (0..count) |i| {
        const key = try std.fmt.allocPrint(alloc, "probe-key-{d}", .{i});
        defer alloc.free(key);
        const body = try std.fmt.allocPrint(alloc,
            \\{{"kind":"llm_provider","provider":"anthropic","model":"claude-{d}"}}
        , .{i});
        defer alloc.free(body);
        try vault.storeJsonPlaintext(alloc, conn, workspace_id, key, body);
    }
}

fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{workspace_id}) catch |err|
        std.log.warn("secret wipe ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, workspace_id);
}

test "credential list reads the vault once regardless of credential count" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek(); // the vault paths below need a process KEK, as serve.run seeds at boot
    try base.seedTenant(db.conn);
    defer teardown(db.conn, WS_ONE);
    defer teardown(db.conn, WS_MANY);

    try seedSecrets(alloc, db.conn, WS_ONE, 1);
    try seedSecrets(alloc, db.conn, WS_MANY, MANY_SECRETS);

    const before_one = try secretScans(db.conn);
    const one = try secret_list.fetchSecretListOnConn(db.conn, alloc, WS_ONE);
    defer freeRows(alloc, one);
    const scans_for_one = (try secretScans(db.conn)) - before_one;

    const before_many = try secretScans(db.conn);
    const many = try secret_list.fetchSecretListOnConn(db.conn, alloc, WS_MANY);
    defer freeRows(alloc, many);
    const scans_for_many = (try secretScans(db.conn)) - before_many;

    try std.testing.expectEqual(@as(usize, 1), one.len);
    try std.testing.expectEqual(MANY_SECRETS, many.len);
    // The point of the workstream: twenty credentials cost what one costs.
    try std.testing.expectEqual(scans_for_one, scans_for_many);
}

test "credential list projects descriptors and never the secret body" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek(); // the vault paths below need a process KEK, as serve.run seeds at boot
    try base.seedTenant(db.conn);
    defer teardown(db.conn, WS_ONE);

    try seedSecrets(alloc, db.conn, WS_ONE, 2);
    const rows = try secret_list.fetchSecretListOnConn(db.conn, alloc, WS_ONE);
    defer freeRows(alloc, rows);

    try std.testing.expectEqual(@as(usize, 2), rows.len);
    for (rows) |r| {
        // Descriptors survive the bulk read; the api_key never appears on the
        // wire struct at all, so the assertion available here is that the
        // non-secret projection still resolves.
        // `llm_provider` is the stored body's discriminator; `provider_key` is
        // the wire name `secret_metadata` projects it to.
        try std.testing.expectEqualStrings("provider_key", r.kind);
        try std.testing.expectEqualStrings("anthropic", r.provider.?);
        try std.testing.expect(std.mem.startsWith(u8, r.model.?, "claude-"));
    }
    // Ordered by key_name, so the page is stable across calls.
    try std.testing.expectEqualStrings("probe-key-0", rows[0].name);
    try std.testing.expectEqualStrings("probe-key-1", rows[1].name);
}

fn freeRows(alloc: std.mem.Allocator, rows: []secret_list.SecretListRow) void {
    for (rows) |r| {
        alloc.free(r.name);
        if (r.provider) |v| alloc.free(v);
        if (r.model) |v| alloc.free(v);
        if (r.base_url) |v| alloc.free(v);
    }
    alloc.free(rows);
}
