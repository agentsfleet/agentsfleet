//! Unit tier for §2 Dimension 2.2 — rate-cache identity is collision-safe.
//!
//! The spec requires this in the exact terms the old encoding failed: *"Rate-cache
//! identity is a collision-safe structured `(provider,model_id)` key, never
//! delimiter concatenation. Migration tests include provider/model strings
//! containing the current `0x1f` separator and prove distinct tuples cannot alias
//! or select another rate."*
//!
//! Why the separator was not safe. `model_rate_cache.zig` joined the pair as
//! `provider ++ 0x1f ++ model` on the reasoning that a unit-separator "never
//! appears in a provider name or model_id". That is a claim about catalogue
//! CONTENT, and nothing anywhere validates it — the catalogue is operator-seeded
//! and admin-mutable. The moment one row carries the byte, two distinct tuples
//! encode to identical bytes:
//!
//!     ("a",      "b\x1fc")  ->  "a\x1fb\x1fc"
//!     ("a\x1fb", "c")       ->  "a\x1fb\x1fc"
//!
//! and the map cannot tell them apart. Whichever loaded last wins, so a request
//! for one model is billed at the other's price. `contextCapForModel` was worse
//! still: it split on the FIRST separator, so for that key it compared against a
//! "model" of `b\x1fc`.
//!
//! These tests are the regression proof for the structured key that replaced it.
//! They are deliberately written against the SEPARATOR BYTE rather than against
//! some arbitrary awkward string — if anyone reintroduces concatenation with any
//! delimiter, the aliasing pair below is the shape that catches it.

const std = @import("std");

const rate_cache = @import("model_rate_cache.zig");

const testing = std.testing;

/// The byte the retired encoding used as its delimiter. Named because it is the
/// subject of these tests, not an incidental constant.
const OLD_SEPARATOR = "\x1f";

/// Two tuples that the retired `provider ++ 0x1f ++ model` encoding mapped to
/// the SAME bytes. Under a structured key they must stay distinct.
const ALIAS_A_PROVIDER = "a";
const ALIAS_A_MODEL = "b" ++ OLD_SEPARATOR ++ "c";
const ALIAS_B_PROVIDER = "a" ++ OLD_SEPARATOR ++ "b";
const ALIAS_B_MODEL = "c";

const RATE_A: i64 = 1_000;
const RATE_B: i64 = 9_999;

fn rate(input: i64) rate_cache.ModelRate {
    return .{
        .input_nanos_per_mtok = input,
        .cached_input_nanos_per_mtok = 0,
        .output_nanos_per_mtok = 0,
        .context_cap_tokens = 100,
    };
}

test "test_rate_cache_key_is_collision_safe" {
    const alloc = testing.allocator;

    // The premise, stated as an assertion so this test cannot quietly stop
    // being about anything: concatenating each pair with the old separator
    // produces identical strings.
    const joined_a = try std.mem.concat(alloc, u8, &.{ ALIAS_A_PROVIDER, OLD_SEPARATOR, ALIAS_A_MODEL });
    defer alloc.free(joined_a);
    const joined_b = try std.mem.concat(alloc, u8, &.{ ALIAS_B_PROVIDER, OLD_SEPARATOR, ALIAS_B_MODEL });
    defer alloc.free(joined_b);
    try testing.expectEqualStrings(joined_a, joined_b);

    // Under the structured key they are two different entries holding two
    // different rates — the aliasing is gone, not merely improbable.
    var cache = rate_cache.Cache.emptyForTest(alloc);
    defer cache.deinit();

    try cache.putForTest(ALIAS_A_PROVIDER, ALIAS_A_MODEL, rate(RATE_A));
    try cache.putForTest(ALIAS_B_PROVIDER, ALIAS_B_MODEL, rate(RATE_B));

    const got_a = cache.lookup(ALIAS_A_PROVIDER, ALIAS_A_MODEL);
    const got_b = cache.lookup(ALIAS_B_PROVIDER, ALIAS_B_MODEL);
    try testing.expect(got_a != null);
    try testing.expect(got_b != null);

    // The load-bearing assertion: each tuple selects ITS OWN rate. Under the
    // retired encoding the second put overwrote the first, and both of these
    // returned RATE_B.
    try testing.expectEqual(RATE_A, got_a.?.input_nanos_per_mtok);
    try testing.expectEqual(RATE_B, got_b.?.input_nanos_per_mtok);
}

test "test_rate_cache_key_is_collision_safe: neither tuple can select the other's rate" {
    const alloc = testing.allocator;
    var cache = rate_cache.Cache.emptyForTest(alloc);
    defer cache.deinit();

    // Only ONE of the aliasing pair is present. A concatenating key would find
    // it under the other's identity too, which is the "selects another rate"
    // half of the spec's requirement.
    try cache.putForTest(ALIAS_A_PROVIDER, ALIAS_A_MODEL, rate(RATE_A));

    try testing.expect(cache.lookup(ALIAS_A_PROVIDER, ALIAS_A_MODEL) != null);
    try testing.expect(cache.lookup(ALIAS_B_PROVIDER, ALIAS_B_MODEL) == null);
}

test "test_rate_cache_key_is_collision_safe: the context-cap scan matches the model field only" {
    const alloc = testing.allocator;
    var cache = rate_cache.Cache.emptyForTest(alloc);
    defer cache.deinit();

    // `contextCapForModel` used to locate the model by splitting the composite
    // key at its first separator. For provider `a\x1fb` that yielded a "model"
    // of `b\x1fc`, so a lookup for the real model `c` missed while a lookup for
    // a model that never existed hit.
    try cache.putForTest(ALIAS_B_PROVIDER, ALIAS_B_MODEL, rate(RATE_B));

    try testing.expectEqual(@as(?u32, 100), cache.contextCapForModel(ALIAS_B_MODEL));
    try testing.expectEqual(@as(?u32, null), cache.contextCapForModel(ALIAS_A_MODEL));
}

test "test_rate_cache_key_is_collision_safe: a separator-free pair is unaffected" {
    const alloc = testing.allocator;
    var cache = rate_cache.Cache.emptyForTest(alloc);
    defer cache.deinit();

    // The ordinary case still behaves: same model name under two providers is
    // two rates, which is the property the old separator existed to provide and
    // which the structured key must not regress.
    try cache.putForTest("anthropic", "claude-opus-4-8", rate(RATE_A));
    try cache.putForTest("pioneer", "claude-opus-4-8", rate(RATE_B));

    try testing.expectEqual(RATE_A, cache.lookup("anthropic", "claude-opus-4-8").?.input_nanos_per_mtok);
    try testing.expectEqual(RATE_B, cache.lookup("pioneer", "claude-opus-4-8").?.input_nanos_per_mtok);
    try testing.expect(cache.lookup("moonshot", "claude-opus-4-8") == null);
}
