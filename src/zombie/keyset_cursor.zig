// Keyset cursor — pure parse/format for keyset-pagination cursors.
//
// Composite form: "{created_at_ms}:{uuid}" — the composite key prevents
// silent skips when multiple rows land on the same millisecond. Kept as
// a leaf module with no DB dependency so it is usable from micro-benchmarks
// (src/zbench_micro.zig) alongside the request path.

const std = @import("std");

pub const Cursor = struct {
    created_at_ms: i64,
    id: []const u8,
};

pub const Error = error{InvalidCursor};

/// Parse "{ts}:{id}" into a Cursor. Borrows from `raw` — no allocation.
pub fn parse(raw: []const u8) Error!Cursor {
    const sep = std.mem.indexOfScalar(u8, raw, ':') orelse return Error.InvalidCursor;
    const ts = std.fmt.parseInt(i64, raw[0..sep], 10) catch return Error.InvalidCursor;
    const id = raw[sep + 1 ..];
    if (id.len == 0) return Error.InvalidCursor;
    return .{ .created_at_ms = ts, .id = id };
}

/// Format a cursor as "{ts}:{id}". Caller owns the returned slice.
pub fn format(alloc: std.mem.Allocator, c: Cursor) ![]u8 {
    return std.fmt.allocPrint(alloc, "{d}:{s}", .{ c.created_at_ms, c.id });
}

test "parse: well-formed cursor" {
    const c = try parse("1744000000000:019abc");
    try std.testing.expectEqual(@as(i64, 1744000000000), c.created_at_ms);
    try std.testing.expectEqualStrings("019abc", c.id);
}

test "parse: missing separator" {
    try std.testing.expectError(Error.InvalidCursor, parse("1744000000000"));
}

test "parse: empty id after separator" {
    try std.testing.expectError(Error.InvalidCursor, parse("1744000000000:"));
}

test "parse: non-numeric ts" {
    try std.testing.expectError(Error.InvalidCursor, parse("abc:019abc"));
}

test "format: round-trip" {
    const alloc = std.testing.allocator;
    const src: Cursor = .{ .created_at_ms = 1744000000000, .id = "019abc" };
    const s = try format(alloc, src);
    defer alloc.free(s);
    const back = try parse(s);
    try std.testing.expectEqual(src.created_at_ms, back.created_at_ms);
    try std.testing.expectEqualStrings(src.id, back.id);
}
