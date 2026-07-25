//! Unit tests for stream-id ordering.
//!
//! `hasDeliverable` needs a live stream, so it is proven in the integration tier.
//! The comparison it rests on is pure, and it is the part most likely to be got
//! wrong: a Redis stream id is the text `<ms>-<seq>`, and its lexicographic order
//! is NOT its chronological order. `"100-0"` sorts before `"99-0"` as text while
//! being the strictly later entry, so a natural string comparison reports "fully
//! delivered" for a stream that has undelivered entries — and the sweeper would
//! then never re-mark the fleet whose event was stranded.
//!
//! These tests exist to make that specific mistake impossible to reintroduce.

const std = @import("std");
const probe = @import("redis_fleet_probe.zig");

const testing = std.testing;

test "a stream id parses into its millisecond and sequence parts" {
    const id = try probe.parseStreamId("1700000000000-5");
    try testing.expectEqual(@as(u64, 1700000000000), id.ms);
    try testing.expectEqual(@as(u64, 5), id.seq);

    // "0-0" is what a fresh group reports as last-delivered-id, and a bare
    // millisecond is how Redis expands a partial id — both must parse.
    const zero = try probe.parseStreamId("0-0");
    try testing.expectEqual(@as(u64, 0), zero.ms);
    try testing.expectEqual(@as(u64, 0), zero.seq);
    const bare = try probe.parseStreamId("1700000000000");
    try testing.expectEqual(@as(u64, 0), bare.seq);
}

test "malformed ids are rejected rather than silently parsed as zero" {
    // Zero is the "nothing delivered" sentinel. Coercing garbage to it would make
    // a broken reply look like a stream that has delivered nothing, so the
    // sweeper would re-mark every fleet on every pass.
    try testing.expectError(error.InvalidCharacter, probe.parseStreamId("abc-1"));
    try testing.expectError(error.InvalidCharacter, probe.parseStreamId("1700-x"));
    try testing.expectError(error.InvalidCharacter, probe.parseStreamId(""));
}

test "ordering is numeric, not lexicographic" {
    // The whole reason StreamId exists. As text, "100-0" < "99-0"; numerically
    // 100 > 99. A string comparison here would invert the verdict and hide
    // undelivered entries.
    const earlier = try probe.parseStreamId("99-0");
    const later = try probe.parseStreamId("100-0");

    try testing.expect(earlier.lessThan(later));
    try testing.expect(!later.lessThan(earlier));
    // Confirm the text order really is the opposite, so this test is proving a
    // live hazard rather than a hypothetical one.
    try testing.expect(std.mem.lessThan(u8, "100-0", "99-0"));

    // Within one millisecond the sequence breaks the tie — the common case for a
    // burst of ingress, and it must stay exact at the top of the range.
    const first = try probe.parseStreamId("1700000000000-1");
    const second = try probe.parseStreamId("1700000000000-2");
    const highest = try probe.parseStreamId("1700000000000-18446744073709551615");
    try testing.expect(first.lessThan(second));
    try testing.expect(second.lessThan(highest));
    try testing.expect(!highest.lessThan(first));
}

test "an id is not less than itself" {
    // This is the fully-delivered case: last-delivered equals last-generated, so
    // `hasDeliverable` must answer false. A strict comparison is required — a
    // `<=` here would re-mark every idle fleet forever and restore the very scan
    // this workstream removes.
    const id = try probe.parseStreamId("1700000000000-7");
    try testing.expect(!id.lessThan(id));
}

