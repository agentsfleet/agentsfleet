//! Opaque base64url cursor for telemetry pagination.
//!
//! Encodes a (recorded_at, id) tuple as a base64url-no-pad token that callers
//! treat as opaque. `makeCursor` builds one from the last row of a page;
//! `parseCursor` decodes one back into the tuple, rejecting malformed input
//! with `error.InvalidCursor`.

const std = @import("std");
const base64 = std.base64.url_safe_no_pad;

pub const ParsedCursor = struct { recorded_at: i64, id: []u8 };

// Max id segment: UUID-format IDs are ≤36 chars; 128 is a generous cap.
const CURSOR_ID_MAX_LEN: usize = 128;

/// Build an opaque base64url cursor token from a (recorded_at, id) pair.
pub fn makeCursor(alloc: std.mem.Allocator, recorded_at: i64, id: []const u8) ![]u8 {
    const plain = try std.fmt.allocPrint(alloc, "{d}:{s}", .{ recorded_at, id });
    defer alloc.free(plain);
    const encoded_len = base64.Encoder.calcSize(plain.len);
    const encoded = try alloc.alloc(u8, encoded_len);
    _ = base64.Encoder.encode(encoded, plain);
    return encoded;
}

/// Decode an opaque base64url cursor token into its components.
/// Caller owns ParsedCursor.id and must free it (or use an arena allocator).
pub fn parseCursor(alloc: std.mem.Allocator, cursor: []const u8) !ParsedCursor {
    // Base64url-decode the opaque token.
    const decoded_len = base64.Decoder.calcSizeForSlice(cursor) catch return error.InvalidCursor;
    const plain = try alloc.alloc(u8, decoded_len);
    defer alloc.free(plain);
    base64.Decoder.decode(plain, cursor) catch return error.InvalidCursor;

    const sep = std.mem.indexOf(u8, plain, ":") orelse return error.InvalidCursor;
    const ts = std.fmt.parseInt(i64, plain[0..sep], 10) catch return error.InvalidCursor;
    const id_slice = plain[sep + 1 ..];
    if (id_slice.len == 0 or id_slice.len > CURSOR_ID_MAX_LEN) return error.InvalidCursor;

    // Dupe id_slice before plain is freed by defer above.
    return .{ .recorded_at = ts, .id = try alloc.dupe(u8, id_slice) };
}

// ── Tests ───────────────────────────────────────────────────────────────────

// Comptime helper: base64url-encode a string literal for use in test inputs.
fn encode64(comptime src: []const u8) [base64.Encoder.calcSize(src.len)]u8 {
    var buf: [base64.Encoder.calcSize(src.len)]u8 = undefined;
    _ = base64.Encoder.encode(&buf, src);
    return buf;
}

test "parseCursor: valid round-trip" {
    const alloc = std.testing.allocator;
    const cursor = try makeCursor(alloc, 1712924400000, "abc123");
    defer alloc.free(cursor);
    const parsed = try parseCursor(alloc, cursor);
    defer alloc.free(parsed.id);
    try std.testing.expectEqual(@as(i64, 1712924400000), parsed.recorded_at);
    try std.testing.expectEqualStrings("abc123", parsed.id);
}

test "parseCursor: rejects invalid base64" {
    // Non-base64url characters must return InvalidCursor.
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, "!!not-valid-base64!!"));
}

test "parseCursor: rejects empty id" {
    const encoded = comptime encode64("1712924400000:");
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}

test "parseCursor: rejects missing separator" {
    const encoded = comptime encode64("1712924400000");
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}

test "parseCursor: rejects oversized id" {
    const big_id = "x" ** 129;
    const encoded = comptime encode64("1712924400000:" ++ big_id);
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}

test "parseCursor: rejects non-integer timestamp" {
    const encoded = comptime encode64("notanumber:abc123");
    try std.testing.expectError(error.InvalidCursor, parseCursor(std.testing.allocator, &encoded));
}
