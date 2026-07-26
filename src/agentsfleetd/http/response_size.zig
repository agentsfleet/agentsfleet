//! Exact encoded size of a JSON response body, computed without building it.
//!
//! Two callers need the number BEFORE the bytes exist, for unrelated reasons:
//!
//!   - `handlers/sensitive_response.zig` sizes an exact-capacity block so the
//!     serialized secret occupies one known region it can zero afterwards;
//!   - the library read paths compare against §3's per-endpoint encoded-body
//!     ceiling, which they must REFUSE (`UZ-LIBRARY-005`) rather than truncate.
//!     A silently short page is worse than a failed one — the caller cannot
//!     tell it is short.
//!
//! `std.Io.Writer.Discarding` counts what a formatter would have written, so
//! this costs one serialization pass and no allocation. That is why the ceiling
//! check does not need to materialise a body it may be about to reject.
//!
//! The options MUST be the ones the real write uses. `emit_null_optional_fields`
//! alone changes the byte count of every row carrying an absent field, so
//! measuring with defaults and writing with anything else compares one body
//! against another body's ceiling.

const std = @import("std");

/// Bytes `std.json` would emit for `value` under `options`.
pub fn encoded(value: anytype, options: std.json.Stringify.Options) !usize {
    var empty: [0]u8 = .{};
    var counter = std.Io.Writer.Discarding.init(&empty);
    try std.json.fmt(value, options).format(&counter.writer);
    return std.math.cast(usize, counter.fullCount()) orelse error.OutOfMemory;
}

pub const CeilingError = error{BodyCeilingExceeded};

/// `encoded`, refusing anything over `ceiling`.
///
/// The comparison lives here rather than inline at each handler for the same
/// reason `pagination.identityMatches` does: a rule spelled at the call site is
/// tested through the call site, which for a 512 KiB ceiling means building a
/// 512 KiB fixture to exercise one `>`. Extracted, the boundary is three cheap
/// assertions — and the boundary is the whole risk. `>` versus `>=` differs on
/// exactly one input, the body whose size EQUALS the ceiling, and no test that
/// does not land on that byte can tell the two apart.
///
/// Inclusive: a body exactly at the ceiling fits. A ceiling is the largest
/// allowed size, not the first forbidden one.
pub fn encodedWithinCeiling(
    value: anytype,
    options: std.json.Stringify.Options,
    ceiling: usize,
) !usize {
    const size = try encoded(value, options);
    if (size > ceiling) return CeilingError.BodyCeilingExceeded;
    return size;
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "encoded: matches the bytes the same options actually serialize" {
    // The only property that matters: the count and the write agree. Asserting
    // against a hand-written literal would pin std.json's formatting instead.
    const value = .{ .a = @as(u32, 1), .b = "two" };
    const opts: std.json.Stringify.Options = .{};

    const written = try std.json.Stringify.valueAlloc(testing.allocator, value, opts);
    defer testing.allocator.free(written);

    try testing.expectEqual(written.len, try encoded(value, opts));
}

test "encoded: options change the count, which is why callers must pass their own" {
    // An absent optional is the case that bites: measured with the default
    // options this body carries `"missing":null`, and written by a handler that
    // omits nulls it does not. Two different sizes for one value.
    const value = .{ .present = @as(u32, 7), .missing = @as(?u32, null) };

    const with_nulls = try encoded(value, .{ .emit_null_optional_fields = true });
    const without_nulls = try encoded(value, .{ .emit_null_optional_fields = false });
    try testing.expect(with_nulls > without_nulls);

    const written = try std.json.Stringify.valueAlloc(
        testing.allocator,
        value,
        .{ .emit_null_optional_fields = false },
    );
    defer testing.allocator.free(written);
    try testing.expectEqual(written.len, without_nulls);
}

test "encodedWithinCeiling: a body exactly at the ceiling is accepted" {
    // The boundary, and the only input that distinguishes `>` from `>=`. An
    // off-by-one here rejects the largest legal page — a 500 on a request that
    // is entirely within spec, and one no under-the-ceiling test can see.
    const value = .{ .models = "abc", .total = @as(u32, 2) };
    const exact = try encoded(value, .{});
    try testing.expectEqual(exact, try encodedWithinCeiling(value, .{}, exact));
}

test "encodedWithinCeiling: one byte over the ceiling is refused, not truncated" {
    // The refusal `UZ-LIBRARY-005` reports. It returns an error rather than a
    // shortened size because a caller cannot distinguish a truncated page from
    // a complete one, so truncation turns a server fault into missing data the
    // client acts on.
    const value = .{ .models = "abc", .total = @as(u32, 2) };
    const exact = try encoded(value, .{});
    try testing.expectError(
        CeilingError.BodyCeilingExceeded,
        encodedWithinCeiling(value, .{}, exact - 1),
    );
}

test "encodedWithinCeiling: a body well under its ceiling returns the real size" {
    // Non-vacuity: the success arm must hand back the MEASURED size, not the
    // ceiling and not zero — the caller records it as the encoded-byte tally,
    // so a wrong value here is a silently wrong measurement rather than a
    // failure anyone would notice.
    const value = .{ .models = "abc", .total = @as(u32, 2) };
    const exact = try encoded(value, .{});
    try testing.expectEqual(exact, try encodedWithinCeiling(value, .{}, exact + 4096));
}

test "encodedWithinCeiling: a zero ceiling admits nothing" {
    // Degenerate guard. Every JSON body is at least two bytes, so a zero
    // ceiling must refuse rather than divide the world into "fits" by accident.
    try testing.expectError(
        CeilingError.BodyCeilingExceeded,
        encodedWithinCeiling(.{ .a = @as(u8, 1) }, .{}, 0),
    );
}

test "encoded: an empty envelope still has a size" {
    // Guards the degenerate page — zero rows is a valid response, and a ceiling
    // comparison against a count of 0 would pass for the wrong reason.
    try testing.expect(try encoded(.{ .models = [_]u8{}, .total = @as(?u8, null) }, .{}) > 0);
}
