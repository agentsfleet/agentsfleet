//! Filter normalization for the bounded library reads (§2).
//!
//! ## What this module still owns
//!
//! One caller-supplied filter reaches these endpoints: `provider=` on
//! `GET /v1/models`. A `q=` search parameter normalized here until it was
//! retired — built, hardened, published, and sent by no client, so it was
//! removed rather than given a consumer.
//!
//! What normalization does here is exactly the part that is correct without
//! Unicode tables, which Zig's standard library does not ship and no dependency
//! here supplies:
//!
//!   - **trim** and **whitespace collapse** — ASCII whitespace only. Safe to do
//!     bytewise on UTF-8 because every byte of a multi-byte sequence is >= 0x80,
//!     so an ASCII space can never appear *inside* a character. That is the
//!     property that makes this loop correct rather than merely usually correct.
//!   - **the byte bound** — measured after trimming, since leading spaces are
//!     not something to reject a caller for.
//!
//! ## Empty means absent
//!
//! A filter that normalizes to nothing is not a filter for the empty string; it
//! is no filter. `?provider=` and `?provider=%20%20` are the same request as
//! omitting it. Returning null rather than an empty slice makes that
//! unrepresentable at the call site instead of relying on every caller to check
//! `.len == 0`.

const std = @import("std");

/// Maximum size of a normalized filter value, in UTF-8 bytes (§2). A hard input
/// bound, not a throttle — the caller maps this to `UZ-LIBRARY-003`.
pub const MAX_QUERY_BYTES: usize = 128;

pub const Error = error{
    /// Normalized value exceeds `MAX_QUERY_BYTES`, or the input is not
    /// well-formed UTF-8. Both are `UZ-LIBRARY-003`: the first because it is an
    /// input bound, the second because Postgres would reject the bytes anyway,
    /// and failing here names the reason instead of surfacing a database error.
    OutOfBounds,
};

/// True for the ASCII whitespace this collapses. Deliberately not
/// `std.ascii.isWhitespace`'s exact set spelled inline — vertical tab and form
/// feed are included because they are whitespace a paste can carry.
fn isAsciiSpace(c: u8) bool {
    return c == ' ' or c == '\t' or c == '\n' or c == '\r' or c == 0x0b or c == 0x0c;
}

/// Trim ASCII whitespace and collapse interior runs to a single space.
///
/// Caller owns the result. Returns null when nothing survives — see the module
/// note on why absent beats empty.
fn squeeze(alloc: std.mem.Allocator, raw: []const u8) !?[]u8 {
    var start: usize = 0;
    var end: usize = raw.len;
    while (start < end and isAsciiSpace(raw[start])) start += 1;
    while (end > start and isAsciiSpace(raw[end - 1])) end -= 1;
    const trimmed = raw[start..end];
    if (trimmed.len == 0) return null;

    var out = try std.ArrayList(u8).initCapacity(alloc, trimmed.len);
    errdefer out.deinit(alloc);

    var in_space = false;
    for (trimmed) |c| {
        if (isAsciiSpace(c)) {
            in_space = true;
            continue;
        }
        if (in_space) {
            try out.append(alloc, ' ');
            in_space = false;
        }
        try out.append(alloc, c);
    }
    return try out.toOwnedSlice(alloc);
}

/// Normalize a `provider=` filter: trim, collapse, ASCII-lowercase.
///
/// ASCII lowercasing is correct here because provider names are catalogue
/// identifiers drawn from an ASCII vocabulary (`anthropic`, `openai`,
/// `moonshot`), matched for equality rather than substring — so there is no
/// half-folded-needle hazard of the kind that made casefolding the retired `q`
/// term a SQL-side job. An unknown provider stays valid and simply matches
/// nothing: the catalogue is operator data, so treating an unrecognised name as
/// a client error would make the API lie about what it knows.
pub fn normalizeProvider(alloc: std.mem.Allocator, raw: ?[]const u8) (Error || std.mem.Allocator.Error)!?[]u8 {
    const text = raw orelse return null;
    const squeezed = (try squeeze(alloc, text)) orelse return null;
    errdefer alloc.free(squeezed);
    if (squeezed.len > MAX_QUERY_BYTES) return Error.OutOfBounds;
    if (!std.unicode.utf8ValidateSlice(squeezed)) return Error.OutOfBounds;
    for (squeezed) |*c| c.* = std.ascii.toLower(c.*);
    return squeezed;
}
