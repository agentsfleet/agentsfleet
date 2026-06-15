//! Shared page/page_size parsing for paginated list endpoints. One source of
//! truth so the fleet + api-key list parsers cannot drift apart, and the bound
//! lives in exactly one place. Sort is endpoint-specific (each list has its own
//! column allowlist) and stays with the individual handler.

const std = @import("std");
const QUERY_PAGE = "page";
const QUERY_PAGE_SIZE = "page_size";

pub const DEFAULT_PAGE_SIZE: i32 = 25;
pub const MAX_PAGE_SIZE: i32 = 100;

pub const PageParams = struct {
    page: i32 = 1,
    page_size: i32 = DEFAULT_PAGE_SIZE,
};

/// Parse + validate `page`/`page_size` from a query map. Fails closed (returns
/// null → the caller maps it to a single 400) on any malformed param:
/// non-numeric, `page` < 1, or `page_size` outside `1..MAX_PAGE_SIZE`. `qs` is
/// any value exposing `get(key) ?[]const u8` — httpz's query map in production,
/// a fake in tests.
pub fn parsePageParams(qs: anytype) ?PageParams {
    var out: PageParams = .{};
    if (qs.get(QUERY_PAGE)) |v| out.page = std.fmt.parseInt(i32, v, 10) catch return null;
    if (qs.get(QUERY_PAGE_SIZE)) |v| out.page_size = std.fmt.parseInt(i32, v, 10) catch return null;
    if (out.page < 1) return null;
    if (out.page_size < 1 or out.page_size > MAX_PAGE_SIZE) return null;
    return out;
}

// ── Tests ────────────────────────────────────────────────────────────────────

const FakeQs = struct {
    const Self = @This();

    page: ?[]const u8 = null,
    page_size: ?[]const u8 = null,
    fn get(self: Self, key: []const u8) ?[]const u8 {
        if (std.mem.eql(u8, key, QUERY_PAGE)) return self.page;
        if (std.mem.eql(u8, key, QUERY_PAGE_SIZE)) return self.page_size;
        return null;
    }
};

test "parsePageParams: absent params yield the defaults" {
    const pp = parsePageParams(FakeQs{}) orelse return error.TestUnexpectedNull;
    try std.testing.expectEqual(@as(i32, 1), pp.page);
    try std.testing.expectEqual(DEFAULT_PAGE_SIZE, pp.page_size);
}

test "parsePageParams: valid numeric params parse through" {
    const pp = parsePageParams(FakeQs{ .page = "3", .page_size = "50" }) orelse return error.TestUnexpectedNull;
    try std.testing.expectEqual(@as(i32, 3), pp.page);
    try std.testing.expectEqual(@as(i32, 50), pp.page_size);
}

test "parsePageParams: page_size at the max bound is accepted" {
    const pp = parsePageParams(FakeQs{ .page_size = "100" }) orelse return error.TestUnexpectedNull;
    try std.testing.expectEqual(MAX_PAGE_SIZE, pp.page_size);
}

test "parsePageParams: non-numeric page or page_size is rejected" {
    try std.testing.expect(parsePageParams(FakeQs{ .page = "abc" }) == null);
    try std.testing.expect(parsePageParams(FakeQs{ .page_size = "12x" }) == null);
}

test "parsePageParams: page below 1 is rejected" {
    try std.testing.expect(parsePageParams(FakeQs{ .page = "0" }) == null);
    try std.testing.expect(parsePageParams(FakeQs{ .page = "-2" }) == null);
}

test "parsePageParams: page_size outside 1..max is rejected" {
    try std.testing.expect(parsePageParams(FakeQs{ .page_size = "0" }) == null);
    try std.testing.expect(parsePageParams(FakeQs{ .page_size = "101" }) == null);
}
