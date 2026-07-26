//! Search-filter normalization for the bounded library reads (§2, §3).
//!
//! ## The split with SQL, and why it is not arbitrary
//!
//! The spec originally said "normalize in the handler", including NFKC and
//! casefold. Zig's standard library ships no Unicode normalization tables and no
//! dependency here supplies them, so an honest NFKC in this file is not
//! available. Postgres has `normalize(text, NFKC)` built in, it is IMMUTABLE, and
//! `lower(normalize(col, NFKC))` is index-eligible — so the comparison, not the
//! handler, is where normalization belongs. Discovery records the amendment.
//!
//! What stays here is exactly the part that is correct without Unicode tables:
//!
//!   - **trim** and **whitespace collapse** — ASCII whitespace only. Safe to do
//!     bytewise on UTF-8 because every byte of a multi-byte sequence is >= 0x80,
//!     so an ASCII space can never appear *inside* a character. That is the
//!     property that makes this loop correct rather than merely usually correct.
//!   - **the byte bound** — measured after trimming, since leading spaces are
//!     not something to reject a caller for.
//!   - **LIKE escaping** — `%` and `_` are wildcards in `LIKE`, so a user
//!     searching for `gpt_4` must not match `gpt-4`. Escaping is a property of
//!     the pattern language, not of Unicode.
//!
//! Casefolding is deliberately NOT done here. Lowercasing only the ASCII range
//! would fold `GPT` but not `ГПТ`, giving a needle that is half-folded while the
//! column is fully folded by `lower()` — matches that depend on which script the
//! user typed in. Both sides are folded in SQL instead, so they agree by
//! construction.
//!
//! ## Empty means absent
//!
//! A filter that normalizes to nothing is not a filter for the empty string; it
//! is no filter. `?q=` and `?q=%20%20` are the same request as omitting `q`.
//! Returning null rather than an empty slice makes that unrepresentable at the
//! call site instead of relying on every caller to check `.len == 0`.

const std = @import("std");

/// Maximum size of a normalized search term, in UTF-8 bytes (§2). A hard input
/// bound, not a throttle — the caller maps this to `UZ-LIBRARY-003`.
pub const MAX_QUERY_BYTES: usize = 128;

/// The `ESCAPE` character paired with every `LIKE` built from `escapeLike`. The
/// SQL side must spell `LIKE $n ESCAPE '\'` or the escaping here is inert.
pub const LIKE_ESCAPE: u8 = '\\';

pub const Error = error{
    /// Normalized term exceeds `MAX_QUERY_BYTES`, or the input is not
    /// well-formed UTF-8. Both are `UZ-LIBRARY-003`: the first because it is an
    /// input bound, the second because Postgres `normalize()` would reject the
    /// bytes anyway, and failing here names the reason instead of surfacing a
    /// database error.
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

/// Normalize a `q=` search term. Null means the filter is absent.
///
/// Rejects a term whose normalized form exceeds `MAX_QUERY_BYTES`, or which is
/// not valid UTF-8. The result is NOT yet LIKE-escaped — `escapeLike` is a
/// separate step because §3's Fleet search and §2's catalogue search share this
/// normalization but build different patterns from it.
pub fn normalizeSearch(alloc: std.mem.Allocator, raw: ?[]const u8) (Error || std.mem.Allocator.Error)!?[]u8 {
    const text = raw orelse return null;
    const squeezed = (try squeeze(alloc, text)) orelse return null;
    errdefer alloc.free(squeezed);

    // Bound the NORMALIZED form: a term that is only long because of repeated
    // spaces has not actually asked for much, and rejecting it would be
    // surprising.
    if (squeezed.len > MAX_QUERY_BYTES) return Error.OutOfBounds;
    if (!std.unicode.utf8ValidateSlice(squeezed)) return Error.OutOfBounds;
    return squeezed;
}

/// Normalize a `provider=` filter: trim, collapse, ASCII-lowercase.
///
/// ASCII lowercasing IS correct here, unlike for `q`: provider names are
/// catalogue identifiers drawn from an ASCII vocabulary (`anthropic`,
/// `openai`, `moonshot`), matched for equality rather than substring. An unknown
/// provider stays valid and simply matches nothing — the catalogue is operator
/// data, so treating an unrecognised name as a client error would make the API
/// lie about what it knows.
pub fn normalizeProvider(alloc: std.mem.Allocator, raw: ?[]const u8) (Error || std.mem.Allocator.Error)!?[]u8 {
    const text = raw orelse return null;
    const squeezed = (try squeeze(alloc, text)) orelse return null;
    errdefer alloc.free(squeezed);
    if (squeezed.len > MAX_QUERY_BYTES) return Error.OutOfBounds;
    if (!std.unicode.utf8ValidateSlice(squeezed)) return Error.OutOfBounds;
    for (squeezed) |*c| c.* = std.ascii.toLower(c.*);
    return squeezed;
}

/// Wrap a normalized term as a literal-substring `LIKE` pattern: `%term%` with
/// every wildcard in `term` escaped.
///
/// The escape character itself is escaped FIRST — doing it after would also
/// escape the backslashes this function just introduced, turning `%` into a
/// literal backslash followed by a live wildcard.
pub fn likeContains(alloc: std.mem.Allocator, term: []const u8) ![]u8 {
    var out = try std.ArrayList(u8).initCapacity(alloc, term.len + 2);
    errdefer out.deinit(alloc);

    try out.append(alloc, '%');
    for (term) |c| {
        if (c == LIKE_ESCAPE or c == '%' or c == '_') try out.append(alloc, LIKE_ESCAPE);
        try out.append(alloc, c);
    }
    try out.append(alloc, '%');
    return try out.toOwnedSlice(alloc);
}
