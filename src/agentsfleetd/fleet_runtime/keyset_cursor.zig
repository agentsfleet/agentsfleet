// Keyset cursor — pure parse/format for keyset-pagination cursors.
//
// Composite form: "{created_at_ms}:{uuid}" — the composite key prevents
// silent skips when multiple rows land on the same millisecond. Kept as
// a leaf module with no DB dependency so it is usable from micro-benchmarks
// (tests/bench/micro.zig) alongside the request path.

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
    return std.fmt.allocPrint(alloc, INT_FORM_FMT, .{ c.created_at_ms, c.id });
}

// ── Sort-aware form ─────────────────────────────────────────────────────────
// A read ordered by something other than a timestamp (the api-keys list sorts
// by key_name) needs the boundary SORT VALUE in the cursor, not only the row
// id. The integer form keeps the legacy "{ts}:{id}" spelling verbatim — every
// previously-issued cursor still parses — and the text form is prefixed and
// base64url-encoded so a value containing ':' cannot corrupt the boundary.

const TEXT_FORM_PREFIX = "s";
/// The legacy spelling both the plain and the integer-sort forms share.
const INT_FORM_FMT = "{d}:{s}";
const b64 = std.base64.url_safe_no_pad;

pub const SortValue = union(enum) {
    ts: i64,
    text: []const u8,
};

pub const SortCursor = struct {
    sort: SortValue,
    id: []const u8,
};

/// Parse either cursor form. The id borrows from `raw`; a text sort value is
/// decoded into `alloc` — pass the request arena. Caller must free the decoded
/// text only if not using an arena.
pub fn parseSort(alloc: std.mem.Allocator, raw: []const u8) !SortCursor {
    const sep = std.mem.indexOfScalar(u8, raw, ':') orelse return Error.InvalidCursor;
    const head = raw[0..sep];
    const rest = raw[sep + 1 ..];
    if (std.mem.eql(u8, head, TEXT_FORM_PREFIX)) {
        const sep2 = std.mem.indexOfScalar(u8, rest, ':') orelse return Error.InvalidCursor;
        const encoded = rest[0..sep2];
        const id = rest[sep2 + 1 ..];
        if (id.len == 0) return Error.InvalidCursor;
        const len = b64.Decoder.calcSizeForSlice(encoded) catch return Error.InvalidCursor;
        const text = try alloc.alloc(u8, len);
        errdefer alloc.free(text);
        b64.Decoder.decode(text, encoded) catch return Error.InvalidCursor;
        return .{ .sort = .{ .text = text }, .id = id };
    }
    const ts = std.fmt.parseInt(i64, head, 10) catch return Error.InvalidCursor;
    if (rest.len == 0) return Error.InvalidCursor;
    return .{ .sort = .{ .ts = ts }, .id = rest };
}

/// Format either cursor form. Caller owns the returned slice.
pub fn formatSort(alloc: std.mem.Allocator, c: SortCursor) ![]u8 {
    switch (c.sort) {
        .ts => |ts| return std.fmt.allocPrint(alloc, INT_FORM_FMT, .{ ts, c.id }),
        .text => |text| {
            const encoded = try alloc.alloc(u8, b64.Encoder.calcSize(text.len));
            defer alloc.free(encoded);
            _ = b64.Encoder.encode(encoded, text);
            return std.fmt.allocPrint(alloc, TEXT_FORM_PREFIX ++ ":{s}:{s}", .{ encoded, c.id });
        },
    }
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

test "test_keyset_cursor_roundtrips_integer_and_text_sort_values" {
    const alloc = std.testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const aa = arena.allocator();

    // Integer sort value round-trips and keeps the legacy spelling.
    const int_src: SortCursor = .{ .sort = .{ .ts = 1744000000000 }, .id = "019abc" };
    const int_encoded = try formatSort(alloc, int_src);
    defer alloc.free(int_encoded);
    try std.testing.expectEqualStrings("1744000000000:019abc", int_encoded);
    const int_back = try parseSort(aa, int_encoded);
    try std.testing.expectEqual(@as(i64, 1744000000000), int_back.sort.ts);
    try std.testing.expectEqualStrings("019abc", int_back.id);

    // Text sort value round-trips even when it contains the separator.
    const text_src: SortCursor = .{ .sort = .{ .text = "prod:key-name" }, .id = "019def" };
    const text_encoded = try formatSort(alloc, text_src);
    defer alloc.free(text_encoded);
    const text_back = try parseSort(aa, text_encoded);
    try std.testing.expectEqualStrings("prod:key-name", text_back.sort.text);
    try std.testing.expectEqualStrings("019def", text_back.id);

    // A previously-issued "{ts}:{id}" cursor parses as the integer form.
    const legacy = try parseSort(aa, "1744000000000:019abc");
    try std.testing.expectEqual(@as(i64, 1744000000000), legacy.sort.ts);
    try std.testing.expectEqualStrings("019abc", legacy.id);
}

test "parseSort: malformed forms are refused" {
    const alloc = std.testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const aa = arena.allocator();
    try std.testing.expectError(Error.InvalidCursor, parseSort(aa, "no-separator"));
    try std.testing.expectError(Error.InvalidCursor, parseSort(aa, "abc:id"));
    try std.testing.expectError(Error.InvalidCursor, parseSort(aa, "123:"));
    try std.testing.expectError(Error.InvalidCursor, parseSort(aa, "s:only-one-part"));
    try std.testing.expectError(Error.InvalidCursor, parseSort(aa, "s:!!notb64!!:id"));
    try std.testing.expectError(Error.InvalidCursor, parseSort(aa, "s:cHJvZA:"));
}
