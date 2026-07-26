//! Integration proof that an ahead migration ledger is refused before
//! migration cleanup can remove its evidence.

const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");
const cmd_common = @import("../cmd/common.zig");
const db = @import("../db/pool.zig");

const TENANT_WORKSPACE_LIST_INDEX_VERSION: i32 = 38;
const WORKSPACE_LIST_INDEX_SQL =
    "CREATE INDEX IF NOT EXISTS idx_workspaces_tenant_created";
const CONCURRENT_INDEX_SQL = "CONCURRENTLY";

fn workspaceListIndexMigrationSql() ![]const u8 {
    const canonical = cmd_common.canonicalMigrations();
    for (canonical) |migration| {
        if (migration.version != TENANT_WORKSPACE_LIST_INDEX_VERSION) continue;
        return migration.sql;
    }
    return error.MissingWorkspaceIndexMigration;
}

fn countVersionRows(conn: anytype, version: i32) !i64 {
    var query = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM audit.schema_migrations WHERE version = $1",
        .{version},
    ));
    defer query.deinit();
    const row = (try query.next()) orelse return error.MissingCount;
    return row.get(i64, 0);
}

fn countFailureRows(conn: anytype, version: i32) !i64 {
    var query = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM audit.schema_migration_failures WHERE version = $1",
        .{version},
    ));
    defer query.deinit();
    const row = (try query.next()) orelse return error.MissingCount;
    return row.get(i64, 0);
}

fn insertRemovedVersion(conn: anytype, version: i32) !bool {
    var query = PgQuery.from(try conn.query(
        \\INSERT INTO audit.schema_migrations (version, applied_at)
        \\VALUES ($1, $2) ON CONFLICT (version) DO NOTHING
        \\RETURNING version
    , .{ version, 0 }));
    defer query.deinit();
    return (try query.next()) != null;
}

test "migration runner refuses a removed ledger version without deleting it" {
    const alloc = std.testing.allocator;
    const probe = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer probe.pool.deinit();
    defer probe.pool.release(probe.conn);

    const canonical = cmd_common.canonicalMigrations();
    const removed_version: i32 = 35;
    const inserted = try insertRemovedVersion(probe.conn, removed_version);
    defer if (inserted) {
        _ = probe.conn.exec(
            "DELETE FROM audit.schema_migrations WHERE version = $1",
            .{removed_version},
        ) catch null;
    };

    try std.testing.expectError(
        error.MigrationSchemaAhead,
        db.runMigrationsRefusingNewer(probe.pool, &canonical),
    );
    try std.testing.expectEqual(
        @as(i64, 1),
        try countVersionRows(probe.conn, removed_version),
    );
}

test "migration runner preserves a noncanonical failure-only record" {
    const alloc = std.testing.allocator;
    const probe = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer probe.pool.deinit();
    defer probe.pool.release(probe.conn);

    const future_version: i32 = 999_991;
    _ = try probe.conn.exec(
        \\INSERT INTO audit.schema_migration_failures
        \\  (version, failed_at, error_text)
        \\VALUES ($1, 0, 'future failure')
        \\ON CONFLICT (version) DO UPDATE SET error_text = EXCLUDED.error_text
    , .{future_version});
    defer {
        _ = probe.conn.exec(
            "DELETE FROM audit.schema_migration_failures WHERE version = $1",
            .{future_version},
        ) catch null;
    }

    const canonical = cmd_common.canonicalMigrations();
    try std.testing.expectError(
        error.MigrationSchemaAhead,
        db.runMigrationsRefusingNewer(probe.pool, &canonical),
    );
    try std.testing.expectEqual(
        @as(i64, 1),
        try countFailureRows(probe.conn, future_version),
    );
}

test "workspace list index migration uses the standard transactional index creation" {
    const sql = try workspaceListIndexMigrationSql();
    try std.testing.expect(std.mem.indexOf(u8, sql, WORKSPACE_LIST_INDEX_SQL) != null);
    try std.testing.expect(std.mem.indexOf(u8, sql, CONCURRENT_INDEX_SQL) == null);
}
