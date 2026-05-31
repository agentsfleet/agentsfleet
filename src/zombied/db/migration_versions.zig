//! Applied-migration-version tracking: a single-query membership set that
//! replaces the per-version `SELECT … WHERE version = $1` (N+1) the migration
//! loop used to issue. Split from `pool_migrations.zig` per RULE FLL.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("pg_query.zig").PgQuery;
const types = @import("pool_types.zig");

const Conn = pg.Conn;
const Migration = types.Migration;

const S_SELECT_APPLIED_VERSIONS = "SELECT version FROM audit.schema_migrations";

/// Upper bound on the canonical migration list. Sizes the stack buffers that
/// hold the applied-version set (single-query membership test) and the orphan
/// reap IN-list, so neither touches the heap. The canonical list is well under
/// this; exceeding it is a programmer error surfaced as error.OutOfMemory.
pub const MAX_TRACKED_MIGRATIONS = 64;

/// In-memory applied-version membership built from a single query. Only versions
/// present in the canonical list are retained, so orphan rows never grow the
/// buffer past `MAX_TRACKED_MIGRATIONS`.
pub const AppliedVersionSet = struct {
    buf: [MAX_TRACKED_MIGRATIONS]i32 = [_]i32{0} ** MAX_TRACKED_MIGRATIONS,
    len: usize = 0,

    /// Fetch applied versions once; keep those the canonical list cares about.
    /// The result is fully consumed and drained before returning, so the caller
    /// can issue further ops on `conn` immediately (pg drain discipline).
    pub fn load(conn: *Conn, migrations: []const Migration) !AppliedVersionSet {
        var self: AppliedVersionSet = .{};
        var result = PgQuery.from(try conn.query(S_SELECT_APPLIED_VERSIONS, .{}));
        defer result.deinit();
        while (try result.next()) |row| {
            const version = try row.get(i32, 0);
            if (!isCanonical(migrations, version)) continue;
            if (self.len == self.buf.len) return error.OutOfMemory;
            self.buf[self.len] = version;
            self.len += 1;
        }
        return self;
    }

    fn isCanonical(migrations: []const Migration, version: i32) bool {
        for (migrations) |m| {
            if (m.version == version) return true;
        }
        return false;
    }

    pub fn contains(self: *const AppliedVersionSet, version: i32) bool {
        return std.mem.indexOfScalar(i32, self.buf[0..self.len], version) != null;
    }
};

test "AppliedVersionSet.contains reflects only the seeded versions" {
    var set = AppliedVersionSet{};
    set.buf[0] = 3;
    set.buf[1] = 7;
    set.len = 2;
    try std.testing.expect(set.contains(3));
    try std.testing.expect(set.contains(7));
    try std.testing.expect(!set.contains(5)); // absent
    try std.testing.expect(!set.contains(0)); // zero-fill is not membership
}

test "AppliedVersionSet.isCanonical matches the canonical migration list" {
    const migrations = [_]Migration{
        .{ .version = 1, .sql = "" },
        .{ .version = 4, .sql = "" },
    };
    try std.testing.expect(AppliedVersionSet.isCanonical(&migrations, 4));
    try std.testing.expect(!AppliedVersionSet.isCanonical(&migrations, 2));
}
