//! Compound keyset cursors and page-limit parsing for bounded list reads.
//!
//! `docs/REST_API_DESIGN_GUIDELINES.md` §3 mandates Stripe-style keyset
//! pagination where the cursor IS a resource id, encoded through
//! `fleet_runtime/keyset_cursor.zig`. That works when a resource id alone
//! reconstructs the sort position. It does not work for these reads: the tenant
//! model registry orders by `(created_at, id)`, the Fleet gallery by
//! `(created_at, tier_rank, id)` across two tables, and both bind the page to a
//! tenant or workspace and to the filters that produced it. A bare id cannot
//! carry any of that.
//!
//! So the cursor is a small struct, and this module is the one place that turns
//! such a struct into an opaque string and back.
//!
//! ## Canonical form
//!
//! Unpadded base64url of UTF-8 JSON, with keys in the payload struct's field
//! declaration order and no extra keys. Canonicity is enforced by RE-ENCODING
//! rather than by a bespoke strict parser: decode, parse permissively,
//! re-encode canonically, and require the result to equal the input byte for
//! byte. Anything a strict parser would reject — reordered keys, extra keys,
//! different number spelling, added whitespace — differs after the round trip
//! and is rejected, in three lines instead of a hand-written scanner.
//!
//! ## What this deliberately does not do
//!
//! No signature. An HMAC over the cursor would fold the shape and identity
//! checks into one signature check and stop a client forging a boundary, but
//! the key would have to outlive a process and be shared across replicas or
//! pagination breaks on every deploy. The gain is small: the cursor carries the
//! tenant or workspace identity, the handler validates it against the
//! authenticated principal, and a forged cursor can therefore only seek within
//! data the caller may already read. Revisit only if a cursor ever carries
//! something the caller cannot otherwise obtain.

const std = @import("std");

const base64 = std.base64.url_safe_no_pad;

/// Cursor payload version. Bumped only when a payload's field set changes
/// incompatibly; a decoded cursor carrying any other value is rejected, which is
/// what stops a deploy from silently reinterpreting yesterday's boundary.
pub const CURSOR_VERSION: u8 = 1;

/// Page size when the caller names none, and the ceiling it may ask for
/// (`docs/REST_API_DESIGN_GUIDELINES.md` §3).
pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 100;

/// A cursor is rejected for exactly two reasons, and callers map them to
/// different codes: `Malformed` is "this is not a cursor I issued"
/// (`UZ-LIBRARY-001`), `VersionMismatch` is "issued by a different payload
/// generation" — also `UZ-LIBRARY-001`, but kept distinct so a deploy-boundary
/// spike is legible in logs rather than hiding inside a generic parse failure.
///
/// Identity mismatch (`UZ-LIBRARY-002`) is NOT here: only the handler knows the
/// authenticated tenant, the requested workspace, and the active filters, so
/// only the handler can compare them.
pub const Error = error{
    Malformed,
    VersionMismatch,
};

/// Encode `value` as an opaque cursor string. Caller owns the result.
///
/// `T` must have a `v: u8` field first; every payload in this codebase defaults
/// it to `CURSOR_VERSION` so an encoder cannot forget to stamp it.
pub fn encode(alloc: std.mem.Allocator, comptime T: type, value: T) ![]u8 {
    const json = try std.json.Stringify.valueAlloc(alloc, value, .{});
    defer alloc.free(json);
    const out = try alloc.alloc(u8, base64.Encoder.calcSize(json.len));
    _ = base64.Encoder.encode(out, json);
    return out;
}

/// Decode an opaque cursor into `T`, rejecting anything not in canonical form.
///
/// Slices in the returned value are allocated from `alloc` and are NOT
/// individually freed — pass a request-scoped allocator, as every handler here
/// does. This mirrors `handlers/library/entry_view.zig`'s leaky decoders.
pub fn decode(alloc: std.mem.Allocator, comptime T: type, raw: []const u8) (Error || std.mem.Allocator.Error)!T {
    const len = base64.Decoder.calcSizeForSlice(raw) catch return Error.Malformed;
    const json = try alloc.alloc(u8, len);
    base64.Decoder.decode(json, raw) catch return Error.Malformed;

    // `ignore_unknown_fields` stays FALSE: an unknown key means the cursor was
    // not issued by this payload version, and accepting it would silently drop
    // a field that constrains the page.
    const parsed = std.json.parseFromSliceLeaky(T, alloc, json, .{
        .allocate = .alloc_always,
    }) catch return Error.Malformed;

    // Canonicity: re-encode and require identical bytes. Reordered keys, extra
    // whitespace, `1.0` for `1` — each survives a permissive parse and each
    // fails here.
    const canonical = try std.json.Stringify.valueAlloc(alloc, parsed, .{});
    defer alloc.free(canonical);
    if (!std.mem.eql(u8, canonical, json)) return Error.Malformed;

    if (parsed.v != CURSOR_VERSION) return Error.VersionMismatch;
    return parsed;
}

/// Does a decoded cursor describe the request now being served?
///
/// A cursor is bound to the query that produced it. `decode` proves only that
/// the bytes are one this service issued; it cannot know the authenticated
/// principal or the active page size, so this is the second half of the check
/// and the difference between `UZ-LIBRARY-001` and `UZ-LIBRARY-002`.
///
/// A free function rather than a method on each payload because every list read
/// applies the identical rule to a differently-shaped cursor, and because a rule
/// spelled at each call site is one that eventually differs at one of them.
/// Extracted so the mapping is unit-testable without an HTTP context.
pub fn identityMatches(cursor_owner: []const u8, request_owner: []const u8, cursor_limit: u32, request_limit: u32) bool {
    return std.mem.eql(u8, cursor_owner, request_owner) and cursor_limit == request_limit;
}

/// Parse a `limit=` query value. Absent or empty yields `DEFAULT_LIMIT`; a
/// non-numeric value, zero, or anything above `MAX_LIMIT` is rejected so the
/// caller can answer `UZ-LIBRARY-003`.
///
/// Out-of-range is an ERROR, not a clamp. Clamping would serve 100 rows to a
/// caller that asked for 1000 and let it believe it had the whole set — the
/// pagination bug that only shows up once a tenant grows past one page.
pub fn parseLimit(raw: ?[]const u8) error{OutOfRange}!u32 {
    const text = raw orelse return DEFAULT_LIMIT;
    if (text.len == 0) return DEFAULT_LIMIT;
    const n = std.fmt.parseInt(u32, text, 10) catch return error.OutOfRange;
    if (n == 0 or n > MAX_LIMIT) return error.OutOfRange;
    return n;
}

// ── tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

/// Mirrors the tenant model registry payload: version first, then the sort key,
/// then the identity and bounds the page was issued under.
const TestCursor = struct {
    v: u8 = CURSOR_VERSION,
    created_at: i64,
    id: []const u8,
    tenant_uuid: []const u8,
    limit: u32,
};

fn sample() TestCursor {
    return .{
        .created_at = 1_745_884_800_000,
        .id = "0195b4ba-8d3a-7f13-8abc-cd0000000002",
        .tenant_uuid = "0195b4ba-8d3a-7f13-8abc-aa0000000002",
        .limit = 50,
    };
}

test "round-trips every field losslessly" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();

    const src = sample();
    const encoded = try encode(alloc, TestCursor, src);
    defer alloc.free(encoded);

    const back = try decode(arena.allocator(), TestCursor, encoded);
    try testing.expectEqual(src.v, back.v);
    try testing.expectEqual(src.created_at, back.created_at);
    try testing.expectEqualStrings(src.id, back.id);
    try testing.expectEqualStrings(src.tenant_uuid, back.tenant_uuid);
    try testing.expectEqual(src.limit, back.limit);
}

test "encodes as unpadded base64url — no '=', '+', or '/'" {
    const alloc = testing.allocator;
    const encoded = try encode(alloc, TestCursor, sample());
    defer alloc.free(encoded);
    try testing.expect(std.mem.indexOfScalar(u8, encoded, '=') == null);
    try testing.expect(std.mem.indexOfScalar(u8, encoded, '+') == null);
    try testing.expect(std.mem.indexOfScalar(u8, encoded, '/') == null);
}

test "rejects a payload whose keys are reordered" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();

    // Same values, `id` before `created_at`. A permissive parse accepts this;
    // the re-encode comparison is what refuses it.
    const reordered =
        \\{"v":1,"id":"a","created_at":1,"tenant_uuid":"t","limit":50}
    ;
    const encoded = try alloc.alloc(u8, base64.Encoder.calcSize(reordered.len));
    defer alloc.free(encoded);
    _ = base64.Encoder.encode(encoded, reordered);

    try testing.expectError(Error.Malformed, decode(arena.allocator(), TestCursor, encoded));
}

test "rejects an extra key the payload does not declare" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();

    const extra =
        \\{"v":1,"created_at":1,"id":"a","tenant_uuid":"t","limit":50,"extra":true}
    ;
    const encoded = try alloc.alloc(u8, base64.Encoder.calcSize(extra.len));
    defer alloc.free(encoded);
    _ = base64.Encoder.encode(encoded, extra);

    try testing.expectError(Error.Malformed, decode(arena.allocator(), TestCursor, encoded));
}

test "rejects a wrong version distinctly from a shape failure" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();

    const v99 =
        \\{"v":99,"created_at":1,"id":"a","tenant_uuid":"t","limit":50}
    ;
    const encoded = try alloc.alloc(u8, base64.Encoder.calcSize(v99.len));
    defer alloc.free(encoded);
    _ = base64.Encoder.encode(encoded, v99);

    try testing.expectError(Error.VersionMismatch, decode(arena.allocator(), TestCursor, encoded));
}

test "rejects malformed base64, non-JSON, and truncated input" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const a = arena.allocator();

    // Not base64url at all.
    try testing.expectError(Error.Malformed, decode(a, TestCursor, "!!!!not-base64!!!!"));
    // Padded base64 is not the canonical encoding this issues.
    try testing.expectError(Error.Malformed, decode(a, TestCursor, "eyJ2Ijox="));
    // Valid base64url, but the plaintext is not JSON.
    const not_json = "aGVsbG8"; // "hello"
    try testing.expectError(Error.Malformed, decode(a, TestCursor, not_json));
    // Empty input carries no boundary.
    try testing.expectError(Error.Malformed, decode(a, TestCursor, "eyJ2IjoxLA"));
}

test "parseLimit: absent and empty both mean the default" {
    try testing.expectEqual(DEFAULT_LIMIT, try parseLimit(null));
    try testing.expectEqual(DEFAULT_LIMIT, try parseLimit(""));
}

test "parseLimit: accepts the inclusive 1..MAX range" {
    try testing.expectEqual(@as(u32, 1), try parseLimit("1"));
    try testing.expectEqual(@as(u32, 50), try parseLimit("50"));
    try testing.expectEqual(MAX_LIMIT, try parseLimit("100"));
}

/// A page size far above `MAX_LIMIT`, as an impatient client would send it.
const WAY_OVER_MAX = "1000";

test "parseLimit: rejects rather than clamps out-of-range" {
    // The load-bearing one: this must NOT silently become MAX_LIMIT, or a
    // caller reads 100 rows believing it received everything.
    try testing.expectError(error.OutOfRange, parseLimit(WAY_OVER_MAX));
    try testing.expectError(error.OutOfRange, parseLimit("101"));
    try testing.expectError(error.OutOfRange, parseLimit("0"));
    try testing.expectError(error.OutOfRange, parseLimit("-1"));
    try testing.expectError(error.OutOfRange, parseLimit("abc"));
    try testing.expectError(error.OutOfRange, parseLimit("12.5"));
}
