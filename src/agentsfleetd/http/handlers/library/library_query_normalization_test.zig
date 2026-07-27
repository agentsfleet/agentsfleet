//! Unit tier for §2 Dimension 2.1 — search-filter normalization.
//!
//! Spec row: *"NFKC, trim, whitespace collapse, casefold; normalized empty means
//! absent; over-128-byte input is `UZ-LIBRARY-003`; LIKE wildcards are escaped so
//! `%` and `_` match literally."*
//!
//! Two of those are asserted somewhere other than where the spec first put them,
//! and the tests say so rather than quietly skipping them:
//!
//!   - **NFKC and casefold happen in SQL**, per the Discovery amendment. Zig
//!     ships no Unicode normalization tables, and half-folding a term in the
//!     handler (ASCII only) while the column is fully folded by `lower()` gives
//!     matches that depend on which script the user typed in. `lower(normalize(
//!     col, NFKC))` on both sides is the fix; these tests cover the ASCII-safe
//!     half that genuinely belongs in Zig.
//!   - **Wildcard escaping is the security-adjacent one.** `_` matches any single
//!     character in `LIKE`, so an unescaped `gpt_4` silently matches `gpt-4`,
//!     `gpt.4`, `gpt 4`. That is a wrong answer presented as a search result.

const std = @import("std");

const ec = @import("../../../errors/error_registry.zig");
const query = @import("query.zig");

const testing = std.testing;

fn expectAbsent(got: ?[]u8) !void {
    try testing.expect(got == null);
}

test "test_library_query_normalization" {
    const alloc = testing.allocator;

    // ── trim ──
    {
        const got = (try query.normalizeSearch(alloc, "  claude  ")).?;
        defer alloc.free(got);
        try testing.expectEqualStrings("claude", got);
    }

    // ── interior runs collapse to a single space ──
    {
        const got = (try query.normalizeSearch(alloc, "claude\t\t opus\n\nlatest")).?;
        defer alloc.free(got);
        try testing.expectEqualStrings("claude opus latest", got);
    }

    // ── case is NOT folded here: SQL folds both sides ──
    // Asserted positively so nobody "fixes" this by adding an ASCII-only
    // toLower, which is the bug the module header describes.
    {
        const got = (try query.normalizeSearch(alloc, "  CLAUDE Opus  ")).?;
        defer alloc.free(got);
        try testing.expectEqualStrings("CLAUDE Opus", got);
    }
}

test "test_library_query_normalization: normalized-empty is absent, not the empty string" {
    const alloc = testing.allocator;

    // A filter that normalizes away is no filter. If these returned an empty
    // slice, a caller doing `if (q) |term|` would filter on "" and match
    // everything or nothing depending on the pattern built from it.
    try expectAbsent(try query.normalizeSearch(alloc, null));
    try expectAbsent(try query.normalizeSearch(alloc, ""));
    try expectAbsent(try query.normalizeSearch(alloc, "   "));
    try expectAbsent(try query.normalizeSearch(alloc, "\t\n\r "));

    try expectAbsent(try query.normalizeProvider(alloc, null));
    try expectAbsent(try query.normalizeProvider(alloc, "  "));
}

test "test_library_query_normalization: over 128 bytes is UZ-LIBRARY-003" {
    const alloc = testing.allocator;

    // Exactly at the bound is accepted — an off-by-one here rejects a legitimate
    // term, and the bound is documented as inclusive.
    const at_bound = try alloc.alloc(u8, query.MAX_QUERY_BYTES);
    defer alloc.free(at_bound);
    @memset(at_bound, 'a');
    {
        const got = (try query.normalizeSearch(alloc, at_bound)).?;
        defer alloc.free(got);
        try testing.expectEqual(query.MAX_QUERY_BYTES, got.len);
    }

    // One past it is not.
    const over = try alloc.alloc(u8, query.MAX_QUERY_BYTES + 1);
    defer alloc.free(over);
    @memset(over, 'a');
    try testing.expectError(query.Error.OutOfBounds, query.normalizeSearch(alloc, over));

    // The bound applies to the NORMALIZED form: padding that collapses away
    // must not push a short term over.
    const padded = try std.mem.concat(alloc, u8, &.{ "   ", at_bound, "   " });
    defer alloc.free(padded);
    {
        const got = (try query.normalizeSearch(alloc, padded)).?;
        defer alloc.free(got);
        try testing.expectEqual(query.MAX_QUERY_BYTES, got.len);
    }

    try testing.expectEqualStrings("UZ-LIBRARY-003", ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS);
}

test "test_library_query_normalization: malformed UTF-8 is rejected, not passed to SQL" {
    const alloc = testing.allocator;

    // Postgres `normalize()` rejects invalid UTF-8, so passing it through would
    // surface as a database error with no useful code. Failing here names it as
    // an input bound instead.
    try testing.expectError(query.Error.OutOfBounds, query.normalizeSearch(alloc, "\xff\xfe"));
    // A truncated multi-byte sequence — the realistic form, from a client that
    // sliced a string by bytes.
    try testing.expectError(query.Error.OutOfBounds, query.normalizeSearch(alloc, "claude \xe2\x82"));
}

test "test_library_query_normalization: multi-byte characters survive collapse intact" {
    const alloc = testing.allocator;

    // The collapse loop is bytewise. It is only correct because every byte of a
    // multi-byte UTF-8 sequence is >= 0x80 and can therefore never equal an
    // ASCII space — if that reasoning were wrong, this test would corrupt a
    // character rather than merely reformat the string.
    const got = (try query.normalizeSearch(alloc, "  клод   опус  ")).?;
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

// LIKE-escaping moved out of Zig entirely: the pattern is built in SQL AFTER
// the NFKC fold (`model_library/sql.zig` `FOLDED_NEEDLE`), because escaping
// before the fold missed compatibility characters that fold INTO wildcards.
// Its contract — `%`, `_`, `\`, and their fullwidth lookalikes match
// literally — is pinned at the integration tier, where the real fold runs:
// `model_library_page_integration_test.zig` (literal `%`, fullwidth `％`).
