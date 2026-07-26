//! Unit tier for §3 Dimension 3.1 — the Fleet gallery seek predicate.
//!
//! Spec row: *"the three-part `created_at`/`tier_rank`/`id` seek orders correctly
//! across every tie combination, `tier_rank` platform=0 before tenant=1."*
//!
//! "Every tie combination" is the phrase that matters. A seek predicate over
//! three keys has three places to get a comparison direction backwards, and each
//! one is invisible until two rows tie on the key before it:
//!
//!   - distinct timestamps hide a broken `tier_rank` comparison entirely;
//!   - distinct timestamps *or* tiers hide a broken `id` comparison;
//!   - and none of the three produces an error when wrong. The page just skips
//!     or repeats rows at its boundary.
//!
//! So the fixtures below are built around ties on purpose, and the ordering is
//! asserted directly rather than inferred from what a query returned.

const std = @import("std");

const keyset = @import("fleet_keyset.zig");

const testing = std.testing;

const OLDER: i64 = 1_745_884_800_000;
const NEWER: i64 = 1_745_884_900_000;

const ID_LOW = "0195b4ba-8d3a-7f13-8abc-000000000001";
const ID_HIGH = "0195b4ba-8d3a-7f13-8abc-000000000002";

fn at(created_at: i64, tier: keyset.Tier, id: []const u8) keyset.Position {
    return .{ .created_at = created_at, .tier_rank = tier.rank(), .id = id };
}

test "test_fleet_keyset_seek_predicate: platform is rank 0 and sorts before tenant" {
    // The rank is the sort position, and platform-before-tenant is the intent.
    // Pinned numerically because ordering on the LABEL happens to give the same
    // answer alphabetically — coincidence that would invert the day a tier named
    // `curated` appears.
    try testing.expectEqual(@as(u8, 0), keyset.Tier.platform.rank());
    try testing.expectEqual(@as(u8, 1), keyset.Tier.tenant.rank());
    try testing.expect(keyset.Tier.platform.rank() < keyset.Tier.tenant.rank());
}

test "test_fleet_keyset_seek_predicate: an unknown tier label is rejected, not defaulted" {
    try testing.expectEqual(keyset.Tier.platform, keyset.Tier.fromLabel("platform").?);
    try testing.expectEqual(keyset.Tier.tenant, keyset.Tier.fromLabel("tenant").?);

    // Defaulting an unrecognised label to `platform` would surface entries from
    // a library the caller may not read.
    try testing.expect(keyset.Tier.fromLabel("curated") == null);
    try testing.expect(keyset.Tier.fromLabel("") == null);
    try testing.expect(keyset.Tier.fromLabel("PLATFORM") == null);
}

test "test_fleet_keyset_seek_predicate: created_at descends — newer sorts first" {
    const newer = at(NEWER, .platform, ID_LOW);
    const older = at(OLDER, .platform, ID_LOW);

    try testing.expectEqual(std.math.Order.lt, keyset.order(newer, older));
    try testing.expectEqual(std.math.Order.gt, keyset.order(older, newer));

    // "After the newer row" is the OLDER row, because the sort descends. This is
    // the comparison most often written backwards.
    try testing.expect(keyset.follows(older, newer));
    try testing.expect(!keyset.follows(newer, older));
}

test "test_fleet_keyset_seek_predicate: on a created_at tie, tier_rank ascends" {
    // Same timestamp — the only case where the tier comparison is reachable.
    const platform = at(NEWER, .platform, ID_LOW);
    const tenant = at(NEWER, .tenant, ID_LOW);

    try testing.expectEqual(std.math.Order.lt, keyset.order(platform, tenant));
    try testing.expectEqual(std.math.Order.gt, keyset.order(tenant, platform));

    // tier_rank ASCENDS, so "after" means a LARGER rank — the opposite direction
    // from the other two keys, and the asymmetry the predicate has to get right.
    try testing.expect(keyset.follows(tenant, platform));
    try testing.expect(!keyset.follows(platform, tenant));
}

test "test_fleet_keyset_seek_predicate: on a created_at AND tier tie, id descends" {
    // Both leading keys tie — the only case where the id comparison decides.
    const high = at(NEWER, .tenant, ID_HIGH);
    const low = at(NEWER, .tenant, ID_LOW);

    try testing.expectEqual(std.math.Order.lt, keyset.order(high, low));
    try testing.expectEqual(std.math.Order.gt, keyset.order(low, high));

    try testing.expect(keyset.follows(low, high));
    try testing.expect(!keyset.follows(high, low));
}

test "test_fleet_keyset_seek_predicate: the boundary is exclusive" {
    // A row never follows itself. An inclusive boundary repeats the row the
    // previous page ended on — the classic keyset duplicate.
    const p = at(NEWER, .tenant, ID_HIGH);
    try testing.expectEqual(std.math.Order.eq, keyset.order(p, p));
    try testing.expect(!keyset.follows(p, p));
}

test "test_fleet_keyset_seek_predicate: a full page sorts into exactly one order" {
    // Every tie combination present at once: two timestamps x two tiers x two
    // ids. Sorting with the comparator and asserting the whole sequence catches
    // a direction error in any single key, because each one is decisive for at
    // least one adjacent pair here.
    var rows = [_]keyset.Position{
        at(OLDER, .tenant, ID_LOW),
        at(NEWER, .platform, ID_HIGH),
        at(OLDER, .platform, ID_HIGH),
        at(NEWER, .tenant, ID_LOW),
        at(NEWER, .platform, ID_LOW),
        at(OLDER, .tenant, ID_HIGH),
        at(NEWER, .tenant, ID_HIGH),
        at(OLDER, .platform, ID_LOW),
    };

    std.mem.sort(keyset.Position, &rows, {}, struct {
        fn lessThan(_: void, a: keyset.Position, b: keyset.Position) bool {
            return keyset.order(a, b) == .lt;
        }
    }.lessThan);

    const want = [_]keyset.Position{
        at(NEWER, .platform, ID_HIGH),
        at(NEWER, .platform, ID_LOW),
        at(NEWER, .tenant, ID_HIGH),
        at(NEWER, .tenant, ID_LOW),
        at(OLDER, .platform, ID_HIGH),
        at(OLDER, .platform, ID_LOW),
        at(OLDER, .tenant, ID_HIGH),
        at(OLDER, .tenant, ID_LOW),
    };

    for (want, rows) |w, got| {
        try testing.expectEqual(w.created_at, got.created_at);
        try testing.expectEqual(w.tier_rank, got.tier_rank);
        try testing.expectEqualStrings(w.id, got.id);
    }

    // And the predicate agrees with that order: every row follows each of its
    // predecessors and none of its successors. This is the property that keeps
    // ORDER BY and the seek in step — they disagree silently, never loudly.
    for (rows, 0..) |row, i| {
        for (rows[0..i]) |before| try testing.expect(keyset.follows(row, before));
        for (rows[i + 1 ..]) |after| try testing.expect(!keyset.follows(row, after));
    }
}
