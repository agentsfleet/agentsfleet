const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("pg_query.zig").PgQuery;
const base = @import("test_fixtures.zig");

test "integration: PgQuery.from wraps conn.query result and next() returns rows" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = PgQuery.from(try db_ctx.conn.query("SELECT 1 AS v", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqual(@as(i32, 1), try row.get(i32, 0));
}

test "integration: PgQuery.next returns null when no rows match" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = PgQuery.from(try db_ctx.conn.query("SELECT 1 WHERE false", .{}));
    defer q.deinit();
    const row = try q.next();
    try std.testing.expect(row == null);
}

test "integration: PgQuery.deinit auto-drains unconsumed rows" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT generate_series(1, 100)",
            .{},
        ));
        _ = try q.next();
        q.deinit();
    }

    var q2 = PgQuery.from(try db_ctx.conn.query("SELECT 42 AS v", .{}));
    defer q2.deinit();
    const row = (try q2.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqual(@as(i32, 42), try row.get(i32, 0));
}

test "integration: PgQuery works with multi-row result iteration" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT generate_series(1, 5)::INTEGER AS v",
        .{},
    ));
    defer q.deinit();

    var count: u32 = 0;
    while (try q.next()) |row| {
        const v = try row.get(i32, 0);
        try std.testing.expect(v >= 1 and v <= 5);
        count += 1;
    }
    try std.testing.expectEqual(@as(u32, 5), count);
}

test "integration: PgQuery.drain is idempotent" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = PgQuery.from(try db_ctx.conn.query("SELECT 1", .{}));
    defer q.deinit();
    _ = try q.next();
    q.drain();
    q.drain();
}

test "integration: PgQuery allows sequential queries on same connection" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q1 = PgQuery.from(try db_ctx.conn.query("SELECT 'first'::TEXT AS v", .{}));
        defer q1.deinit();
        const row = (try q1.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqualStrings("first", try row.get([]const u8, 0));
    }

    {
        var q2 = PgQuery.from(try db_ctx.conn.query("SELECT 'second'::TEXT AS v", .{}));
        defer q2.deinit();
        const row = (try q2.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqualStrings("second", try row.get([]const u8, 0));
    }
}

test "integration: PgQuery works with early return (defer deinit covers cleanup)" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const result = blk: {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT generate_series(1, 10)::INTEGER AS v",
            .{},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse break :blk @as(i32, 0);
        break :blk try row.get(i32, 0);
    };
    try std.testing.expectEqual(@as(i32, 1), result);

    var q2 = PgQuery.from(try db_ctx.conn.query("SELECT 99 AS v", .{}));
    defer q2.deinit();
    const row = (try q2.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqual(@as(i32, 99), try row.get(i32, 0));
}

test "integration: PgQuery with nullable column" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = PgQuery.from(try db_ctx.conn.query("SELECT NULL::TEXT AS v", .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    const val = try row.get(?[]const u8, 0);
    try std.testing.expect(val == null);
}

test "integration: PgQuery with RETURNING clause from DML" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE pgquery_test (id SERIAL PRIMARY KEY, val TEXT NOT NULL)
    , .{});

    var q = PgQuery.from(try db_ctx.conn.query(
        "INSERT INTO pgquery_test (val) VALUES ('hello') RETURNING id, val",
        .{},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    const id = try row.get(i32, 0);
    try std.testing.expect(id > 0);
    try std.testing.expectEqualStrings("hello", try row.get([]const u8, 1));
}
