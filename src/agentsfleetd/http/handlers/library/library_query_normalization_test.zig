//! Unit tier for §2 and §4 — filter normalization on the bounded library reads.
//!
//! **Retargeted when the `q=` search parameter was retired.** These cases were
//! written against that parameter. The normalization they cover did not go
//! with it: `provider=` runs through the same `squeeze` — the same bytewise trim
//! and collapse, the same byte bound, the same UTF-8 rejection — so the cases are
//! repointed onto the surviving caller rather than deleted. Deleting them would
//! have dropped live coverage on the pretext that its original subject left.
//!
//! What genuinely retired with `q` is named here so nobody looks for it:
//!
//!   - **The "case is NOT folded here" case.** It asserted that `q` reached SQL
//!     unfolded, because half-folding a search term in Zig (ASCII only) against
//!     a column fully folded by `lower()` gives matches that depend on which
//!     script the user typed in. `provider` is folded here deliberately — it is
//!     an equality match on an ASCII vocabulary, so the hazard does not apply.
//!   - **Wildcard escaping.** `_` matches any single character in `LIKE`, so an
//!     unescaped `gpt_4` silently matched `gpt-4`. With no `LIKE` pattern left on
//!     these reads, there is nothing to escape.

const std = @import("std");

const ec = @import("../../../errors/error_registry.zig");
const pagination = @import("../../pagination.zig");
const query = @import("query.zig");

const testing = std.testing;

fn expectAbsent(got: ?[]u8) !void {
    try testing.expect(got == null);
}

test "test_library_query_normalization" {
    const alloc = testing.allocator;

    // ── trim ──
    {
        const got = (try query.normalizeProvider(alloc, "  anthropic  ")).?;
        defer alloc.free(got);
        try testing.expectEqualStrings("anthropic", got);
    }

    // ── interior runs collapse to a single space ──
    {
        const got = (try query.normalizeProvider(alloc, "open\t\t router\n\nlabs")).?;
        defer alloc.free(got);
        try testing.expectEqualStrings("open router labs", got);
    }
}

test "test_library_query_normalization: normalized-empty is absent, not the empty string" {
    const alloc = testing.allocator;

    // A filter that normalizes away is no filter. If these returned an empty
    // slice, a caller doing `if (provider) |p|` would filter on "" and match
    // everything or nothing depending on the predicate built from it.
    try expectAbsent(try query.normalizeProvider(alloc, null));
    try expectAbsent(try query.normalizeProvider(alloc, ""));
    try expectAbsent(try query.normalizeProvider(alloc, "   "));
    try expectAbsent(try query.normalizeProvider(alloc, "\t\n\r "));
}

test "test_library_query_normalization: over 128 bytes is UZ-LIBRARY-003" {
    const alloc = testing.allocator;

    // Exactly at the bound is accepted — an off-by-one here rejects a legitimate
    // value, and the bound is documented as inclusive.
    const at_bound = try alloc.alloc(u8, query.MAX_QUERY_BYTES);
    defer alloc.free(at_bound);
    @memset(at_bound, 'a');
    {
        const got = (try query.normalizeProvider(alloc, at_bound)).?;
        defer alloc.free(got);
        try testing.expectEqual(query.MAX_QUERY_BYTES, got.len);
    }

    // One past it is not.
    const over = try alloc.alloc(u8, query.MAX_QUERY_BYTES + 1);
    defer alloc.free(over);
    @memset(over, 'a');
    try testing.expectError(query.Error.OutOfBounds, query.normalizeProvider(alloc, over));

    // The bound applies to the NORMALIZED form: padding that collapses away
    // must not push a short value over.
    const padded = try std.mem.concat(alloc, u8, &.{ "   ", at_bound, "   " });
    defer alloc.free(padded);
    {
        const got = (try query.normalizeProvider(alloc, padded)).?;
        defer alloc.free(got);
        try testing.expectEqual(query.MAX_QUERY_BYTES, got.len);
    }

    try testing.expectEqualStrings("UZ-LIBRARY-003", ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS);
}

test "test_library_query_normalization: malformed UTF-8 is rejected, not passed to SQL" {
    const alloc = testing.allocator;

    // Postgres rejects invalid UTF-8, so passing it through would surface as a
    // database error with no useful code. Failing here names it as an input
    // bound instead.
    try testing.expectError(query.Error.OutOfBounds, query.normalizeProvider(alloc, "\xff\xfe"));
    // A truncated multi-byte sequence — the realistic form, from a client that
    // sliced a string by bytes.
    try testing.expectError(query.Error.OutOfBounds, query.normalizeProvider(alloc, "anthropic \xe2\x82"));
}

test "test_library_query_normalization: multi-byte characters survive collapse intact" {
    const alloc = testing.allocator;

    // The collapse loop is bytewise. It is only correct because every byte of a
    // multi-byte UTF-8 sequence is >= 0x80 and can therefore never equal an
    // ASCII space — if that reasoning were wrong, this test would corrupt a
    // character rather than merely reformat the string.
    //
    // The ASCII-only lowercase pass leaves these code points untouched, which is
    // the same property under test: a bytewise loop that does not straddle a
    // character boundary.
    const got = (try query.normalizeProvider(alloc, "  клод   опус  ")).?;
    defer alloc.free(got);
    try testing.expectEqualStrings("клод опус", got);
    try testing.expect(std.unicode.utf8ValidateSlice(got));
}

test "test_library_query_normalization: provider lowercases and stays open-vocabulary" {
    const alloc = testing.allocator;

    {
        const got = (try query.normalizeProvider(alloc, "  ANTHROPIC ")).?;
        defer alloc.free(got);
        try testing.expectEqualStrings("anthropic", got);
    }

    // An unknown provider is VALID and simply matches nothing. The catalogue is
    // operator-seeded, so rejecting an unrecognised name would make the API
    // assert something about the catalogue it cannot know.
    {
        const got = (try query.normalizeProvider(alloc, "NotARealProvider")).?;
        defer alloc.free(got);
        try testing.expectEqualStrings("notarealprovider", got);
    }
}

test "test_library_limit_bound_survives_search_retirement" {
    // Dimension 4.2. `UZ-LIBRARY-003` covered two causes: a search term over its
    // byte bound, and a `limit` outside 1..100. §4 retired the first. The code
    // keeps its identifier and its registry row for the second — narrowed, not
    // deleted — so a caller who oversteps `limit` still gets a named error rather
    // than a generic one.
    try testing.expectError(error.OutOfRange, pagination.parseLimit("0"));
    try testing.expectError(error.OutOfRange, pagination.parseLimit("101"));
    try testing.expectError(error.OutOfRange, pagination.parseLimit("not-a-number"));

    try testing.expectEqual(@as(u32, 1), try pagination.parseLimit("1"));
    try testing.expectEqual(pagination.MAX_LIMIT, try pagination.parseLimit("100"));
    try testing.expectEqual(pagination.DEFAULT_LIMIT, try pagination.parseLimit(null));

    try testing.expectEqualStrings("UZ-LIBRARY-003", ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS);

    // The retired half must not be reachable: `query` no longer exposes a search
    // normalizer, so no code path can raise this code for a search bound.
    try testing.expect(!@hasDecl(query, "normalizeSearch"));
}
