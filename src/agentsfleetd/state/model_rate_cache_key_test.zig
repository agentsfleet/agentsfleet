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

/// The key policy under test. `hash` and `eql` ARE the collision guarantee — the
/// table only ever distinguishes two keys through them — so these assert against
/// the policy directly rather than through cache hits. A behavioural test would
/// only ever sample the pairs it thought to insert; this covers the rule itself.
const ctx = rate_cache.RateKeyContext{};

fn key(provider: []const u8, model: []const u8) rate_cache.RateKey {
    return .{ .provider = provider, .model = model };
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

    // Under the structured key they are two different keys. `eql` is the
    // load-bearing one — it is what the table consults to decide whether a
    // lookup has found its entry — so a false here is the whole guarantee.
    const a = key(ALIAS_A_PROVIDER, ALIAS_A_MODEL);
    const b = key(ALIAS_B_PROVIDER, ALIAS_B_MODEL);
    try testing.expect(!ctx.eql(a, b));

    // And they land in different buckets, so neither displaces the other under
    // capacity pressure. Equality alone would keep them correct but co-located.
    try testing.expect(ctx.hash(a) != ctx.hash(b));
}

test "test_rate_cache_key_is_collision_safe: the length prefix separates equal concatenations" {
    // ("ab","c") and ("a","bc") concatenate to the same bytes. Folding each
    // field's LENGTH in before its bytes is what keeps them apart in the hash;
    // without it `eql` would still be correct, but every such pair would collide
    // into one four-entry bucket and evict each other on every access.
    const ab_c = key("ab", "c");
    const a_bc = key("a", "bc");
    try testing.expect(!ctx.eql(ab_c, a_bc));
    try testing.expect(ctx.hash(ab_c) != ctx.hash(a_bc));
}

test "test_rate_cache_key_is_collision_safe: the same model under two providers stays distinct" {
    // The ordinary case the old separator existed to provide, and which the
    // structured key must not regress: claude-opus-4-8 on anthropic must not
    // select the pioneer rate, which is a different price for the same name.
    const anthropic = key("anthropic", "claude-opus-4-8");
    const pioneer = key("pioneer", "claude-opus-4-8");
    try testing.expect(!ctx.eql(anthropic, pioneer));
    try testing.expect(ctx.hash(anthropic) != ctx.hash(pioneer));
    // Identity is reflexive on equal bytes held in DIFFERENT slices — the table
    // compares content, never slice addresses.
    var owned: [9]u8 = "anthropic".*;
    try testing.expect(ctx.eql(anthropic, key(&owned, "claude-opus-4-8")));
}

test "test_rate_cache_key_is_collision_safe: a pair too long for the retired buffer still resolves" {
    // The previous encoding built its key in a 512-byte buffer and SKIPPED any
    // pair that overflowed it — at load and at lookup — so a long provider or
    // model was a permanent miss that billing read as "no rate". A structured
    // key has no buffer to overflow.
    const alloc = testing.allocator;
    const long_provider = try alloc.alloc(u8, 600);
    defer alloc.free(long_provider);
    @memset(long_provider, 'p');

    const long = key(long_provider, "m");
    try testing.expect(ctx.eql(long, key(long_provider, "m")));
    try testing.expect(!ctx.eql(long, key(long_provider, "n")));
}
