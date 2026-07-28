//! The two derived identities a §2 catalogue page carries: the cursor that
//! resumes it, and the cache key that names its response.
//!
//! Both are built from the same selectors, and neither stores them in the clear.
//!
//! ## Why the cache key is an HMAC rather than the selectors themselves
//!
//! §4 forbids raw selectors from entering observable cache keys. A key built by
//! concatenating `provider`, `starting_after` and `limit` would put every
//! selector a tenant sent into a structure that gets dumped by heap
//! inspection, printed by a debug handler someone adds later, or exported by a
//! future cache-stats gauge. A digest under a process-random key is not
//! reversible into those terms by anything holding the key list, and the key
//! never leaves this process.
//!
//! The reasoning outlives the `q` selector that motivated it, which was retired
//! as an unused parameter. `provider` is still a caller-supplied value, and the
//! next selector added will be too.
//!
//! **No tenant is mixed in.** That is Invariant 6 and it is deliberate: the
//! catalogue payload must be byte-identical for every authorized caller, so a
//! tenant-varying key would hide a cross-tenant leak behind a partition instead
//! of exposing it. If a tenant-varying field ever reaches the payload, a
//! tenant-free key is what makes that a visible bug.
//!
//! The revision rides the key in the clear, as a separate field rather than
//! inside the digest. It is not a secret — it is a monotone counter — and
//! keeping it structural is what lets a whole generation age out coherently.

const std = @import("std");
const common = @import("common");

const pagination = @import("../../pagination.zig");
const mlc = @import("../../../state/model_library_cache.zig");

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

/// The boundary a catalogue page resumes from, in the spec's fixed key order.
///
/// `id` is the row's `uid`, not its `model_id`. Normalization is many-to-one, so
/// the (provider, model_id) pair is not unique once folded and cannot serve as
/// the final tiebreak; the uid can, and it rides here opaquely without ever
/// appearing in a response body.
///
/// `provider` is carried so the cursor is bound to the query that issued it —
/// resuming under a different filter would paginate a set the caller never asked
/// for, which is `UZ-LIBRARY-002`.
///
/// `q` was carried here for the same reason until the search parameter was
/// retired. Its removal changes this payload's field set incompatibly, which is
/// precisely what `pagination.CURSOR_VERSION` exists to signal — the version is
/// bumped in the same change, so a cursor issued before the deploy is rejected as
/// `VersionMismatch` rather than reinterpreted against the shorter shape.
pub const Cursor = struct {
    v: u8 = pagination.CURSOR_VERSION,
    display_key: []const u8,
    vendor_key: []const u8,
    id: []const u8,
    provider: ?[]const u8,
    limit: u32,
};

/// Field markers, so a present-but-empty selector and an absent one absorb
/// differently. Same reasoning as `http/etag.zig`'s field encoding.
const FIELD_NULL = [_]u8{0};
const FIELD_PRESENT = [_]u8{1};

/// Process-random HMAC key, derived once on first use.
var g_key: [HmacSha256.key_length]u8 = undefined;
var g_key_ready = std.atomic.Value(bool).init(false);
var g_key_lock: common.Mutex = .{};

/// The per-process key. Derived lazily rather than at boot so this module has no
/// startup ordering requirement; the double-check keeps the common path a single
/// atomic load.
///
/// A failed draw falls back to a zero key rather than refusing to serve. This is
/// a cache key, not an authenticator: a predictable key still hides raw
/// selectors behind a digest, and the only thing an attacker gains by predicting
/// it is the ability to confirm a guess about which selectors are cached — which
/// tells them nothing they could not learn by issuing the query. Refusing to
/// serve the catalogue over a failed `getrandom` would be the worse trade.
/// Mirrors `observability/trace.zig`.
fn hmacKey() *const [HmacSha256.key_length]u8 {
    if (g_key_ready.load(.acquire)) return &g_key; // safe because: pairs with the release-store below.
    g_key_lock.lock();
    defer g_key_lock.unlock();
    if (!g_key_ready.load(.acquire)) { // safe because: re-checked under the lock.
        common.secureRandomBytes(&g_key) catch @memset(&g_key, 0);
        g_key_ready.store(true, .release); // safe because: publishes the key written above.
    }
    return &g_key;
}

/// Absorb one optional selector, length-prefixed.
///
/// Without the prefix, `("ab", "c")` and `("a", "bc")` would produce the same
/// digest — two different queries sharing one cache entry, which is a wrong hit
/// rather than a miss.
fn absorb(mac: *HmacSha256, field: ?[]const u8) void {
    const f = field orelse {
        mac.update(&FIELD_NULL);
        return;
    };
    mac.update(&FIELD_PRESENT);
    var len: [8]u8 = undefined;
    std.mem.writeInt(u64, &len, @intCast(f.len), .big);
    mac.update(&len);
    mac.update(f);
}

/// The response-cache key for one page request at one catalogue generation.
///
/// `provider` is the NORMALIZED form, not the raw query value: two requests that
/// normalize to the same filter are the same page and must share a cache entry,
/// or the cache misses on every spelling variation.
pub fn cacheKey(
    revision: i64,
    provider: ?[]const u8,
    starting_after: ?[]const u8,
    limit: u32,
) mlc.Key {
    var mac = HmacSha256.init(hmacKey());
    absorb(&mac, provider);
    absorb(&mac, starting_after);

    var limit_bytes: [4]u8 = undefined;
    std.mem.writeInt(u32, &limit_bytes, limit, .big);
    mac.update(&limit_bytes);

    var digest: [mlc.DIGEST_LEN]u8 = undefined;
    mac.final(&digest);
    return .{ .revision = @bitCast(revision), .digest = digest };
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

const REV: i64 = 7;
const LIMIT: u32 = 50;

test "the same selectors at the same revision give the same key" {
    const a = cacheKey(REV, "anthropic", null, LIMIT);
    const b = cacheKey(REV, "anthropic", null, LIMIT);
    try testing.expectEqual(a.revision, b.revision);
    try testing.expectEqualSlices(u8, &a.digest, &b.digest);
}

test "the revision is structural, not hashed into the digest" {
    // Two generations of the same query differ in the field the cache compares
    // first, so a whole generation ages out together rather than entry by entry.
    const a = cacheKey(REV, "anthropic", null, LIMIT);
    const b = cacheKey(REV + 1, "anthropic", null, LIMIT);
    try testing.expect(a.revision != b.revision);
    try testing.expectEqualSlices(u8, &a.digest, &b.digest);
}

test "every selector changes the digest" {
    const base = cacheKey(REV, "anthropic", null, LIMIT);
    const other_provider = cacheKey(REV, "openai", null, LIMIT);
    const with_cursor = cacheKey(REV, "anthropic", "abc", LIMIT);
    const other_limit = cacheKey(REV, "anthropic", null, LIMIT + 1);

    try testing.expect(!std.mem.eql(u8, &base.digest, &other_provider.digest));
    try testing.expect(!std.mem.eql(u8, &base.digest, &with_cursor.digest));
    try testing.expect(!std.mem.eql(u8, &base.digest, &other_limit.digest));
}

test "absent and empty are distinct selectors" {
    // `?provider=` normalizes to absent, so these should never both occur — but
    // if a caller ever reaches here with an empty string, it must not collide
    // with "no filter at all" and serve the unfiltered page.
    const absent = cacheKey(REV, null, null, LIMIT);
    const empty = cacheKey(REV, "", null, LIMIT);
    try testing.expect(!std.mem.eql(u8, &absent.digest, &empty.digest));
}

test "field boundaries cannot be shifted between selectors" {
    // Without length prefixes these two collide: one query's tail becomes the
    // next query's head, and two different pages share a cache entry.
    const a = cacheKey(REV, "ab", "c", LIMIT);
    const b = cacheKey(REV, "a", "bc", LIMIT);
    try testing.expect(!std.mem.eql(u8, &a.digest, &b.digest));
}

test "no raw selector survives into the key" {
    // The §4 rule, asserted rather than assumed: the digest is the only place a
    // selector reaches, and it must not contain the term's bytes.
    const needle = "supersecret-provider-name";
    const key = cacheKey(REV, needle, null, LIMIT);
    try testing.expect(std.mem.indexOf(u8, &key.digest, needle) == null);
}

test "the cursor payload declares the spec's fixed key order" {
    // The codec enforces canonical form by re-encoding, so field ORDER is part
    // of the wire format rather than a formatting detail. A reordered struct
    // silently invalidates every cursor already issued.
    const fields = @typeInfo(Cursor).@"struct".fields;
    const expected = [_][]const u8{ "v", "display_key", "vendor_key", "id", "provider", "limit" };
    try testing.expectEqual(expected.len, fields.len);
    inline for (fields, expected) |field, want| {
        try testing.expectEqualStrings(want, field.name);
    }
}

test "test_library_reads_ignore_retired_search_param: no cursor field carries a search term" {
    // Dimension 4.1. The retired `q` must not survive anywhere in the payload —
    // a leftover field would keep binding cursors to a filter no caller can set.
    const fields = @typeInfo(Cursor).@"struct".fields;
    inline for (fields) |field| {
        try testing.expect(!std.mem.eql(u8, field.name, "q"));
    }
}
