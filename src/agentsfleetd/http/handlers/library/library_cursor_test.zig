//! Unit tier for the compound keyset cursor (§Test Specification, rows 1.1 /
//! 2.1 / 3.1 share this one).
//!
//! `http/pagination.zig` carries tests for its own mechanics. This file asserts
//! the thing those cannot: that each rejection class lands on the EXACT public
//! error code the spec's §Error Contracts promises, and that the two codes stay
//! distinguishable.
//!
//! That distinction is a security property, not tidiness. `UZ-LIBRARY-001` means
//! "this is not a cursor I issued" — a truncated URL, a stale link, a client
//! composing its own. `UZ-LIBRARY-002` means "this IS a cursor I issued, for a
//! different tenant or a different page size". Folding them into one code buries
//! a cross-tenant replay attempt in the same signal as a copy-paste error, and
//! nothing downstream can separate them again.

const std = @import("std");

const ec = @import("../../../errors/error_registry.zig");
const pagination = @import("../../pagination.zig");

const base64 = std.base64.url_safe_no_pad;
const testing = std.testing;

/// The tenant registry payload shape: version first, then the sort key, then the
/// identity and bounds the page was issued under. Field ORDER is load-bearing —
/// canonicity is enforced by re-encoding, so a reordered declaration would
/// change which byte strings are accepted.
const Cursor = struct {
    v: u8 = pagination.CURSOR_VERSION,
    created_at: i64,
    id: []const u8,
    tenant_uuid: []const u8,
    limit: u32,
};

const TENANT = "0195b4ba-8d3a-7f13-8abc-aa0000000002";
const OTHER_TENANT = "0195b4ba-8d3a-7f13-8abc-aa0000000099";
const ROW_ID = "0195b4ba-8d3a-7f13-8abc-cd0000000002";
const LIMIT: u32 = 50;

fn sample() Cursor {
    return .{ .created_at = 1_745_884_800_000, .id = ROW_ID, .tenant_uuid = TENANT, .limit = LIMIT };
}

/// base64url-encode a literal JSON body, so a test can hand `decode` bytes that
/// `encode` would never produce.
fn encodeRaw(alloc: std.mem.Allocator, json: []const u8) ![]u8 {
    const out = try alloc.alloc(u8, base64.Encoder.calcSize(json.len));
    _ = base64.Encoder.encode(out, json);
    return out;
}

/// The code a decode failure must surface. Both codec errors are the same public
/// code by design: a client cannot act differently on "bad base64" than on
/// "wrong version", and both mean "do not reuse this link".
fn codeForDecodeError(err: pagination.Error) []const u8 {
    return switch (err) {
        pagination.Error.Malformed, pagination.Error.VersionMismatch => ec.ERR_LIBRARY_CURSOR_MALFORMED,
    };
}

test "test_library_cursor_codec_roundtrip" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();

    // ── canonical encode/decode is lossless across every field and type ──
    const src = sample();
    const encoded = try pagination.encode(alloc, Cursor, src);
    defer alloc.free(encoded);

    const back = try pagination.decode(arena.allocator(), Cursor, encoded);
    try testing.expectEqual(src.v, back.v);
    try testing.expectEqual(src.created_at, back.created_at);
    try testing.expectEqualStrings(src.id, back.id);
    try testing.expectEqualStrings(src.tenant_uuid, back.tenant_uuid);
    try testing.expectEqual(src.limit, back.limit);

    // The wire form is opaque and URL-safe: it must survive being pasted into a
    // query string without escaping.
    try testing.expect(std.mem.indexOfScalar(u8, encoded, '=') == null);
    try testing.expect(std.mem.indexOfScalar(u8, encoded, '+') == null);
    try testing.expect(std.mem.indexOfScalar(u8, encoded, '/') == null);
}

test "test_library_cursor_codec_roundtrip: every malformed shape is UZ-LIBRARY-001" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const a = arena.allocator();

    // Each of these survives a permissive JSON parse. The re-encode comparison
    // is the only thing that refuses them, which is exactly why they are worth
    // pinning: a hand-written strict scanner would have to enumerate them, and
    // would eventually miss one.
    const non_canonical = [_][]const u8{
        // keys reordered
        \\{"v":1,"id":"a","created_at":1,"tenant_uuid":"t","limit":50}
        ,
        // an extra key the payload does not declare
        \\{"v":1,"created_at":1,"id":"a","tenant_uuid":"t","limit":50,"extra":true}
        ,
        // a number spelled differently than it re-encodes
        \\{"v":1,"created_at":1.0,"id":"a","tenant_uuid":"t","limit":50}
        ,
        // inserted whitespace
        \\{"v": 1, "created_at": 1, "id": "a", "tenant_uuid": "t", "limit": 50}
        ,
        // a declared field missing entirely
        \\{"v":1,"created_at":1,"id":"a","limit":50}
        ,
    };

    for (non_canonical) |json| {
        const enc = try encodeRaw(a, json);
        const err = pagination.decode(a, Cursor, enc);
        try testing.expectError(pagination.Error.Malformed, err);
        try testing.expectEqualStrings(
            ec.ERR_LIBRARY_CURSOR_MALFORMED,
            codeForDecodeError(pagination.Error.Malformed),
        );
    }

    // Not a cursor at all, in the shapes a broken link actually arrives as.
    const garbage = [_][]const u8{
        "!!!!not-base64!!!!",
        "eyJ2Ijox=", // padded — not the unpadded form this issues
        "aGVsbG8", // valid base64url, plaintext "hello", not JSON
        "", // empty carries no boundary
    };
    for (garbage) |raw| {
        try testing.expectError(pagination.Error.Malformed, pagination.decode(a, Cursor, raw));
    }
}

test "test_library_cursor_codec_roundtrip: a wrong version is distinct from a bad shape" {
    const alloc = testing.allocator;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const a = arena.allocator();

    // Canonical in every respect except `v`. This must NOT collapse into
    // Malformed internally: a version spike at a deploy boundary is a legible
    // operational signal, and it disappears if it is indistinguishable from a
    // client sending junk.
    const v99 =
        \\{"v":99,"created_at":1,"id":"a","tenant_uuid":"t","limit":50}
    ;
    const enc = try encodeRaw(a, v99);
    try testing.expectError(pagination.Error.VersionMismatch, pagination.decode(a, Cursor, enc));

    // Publicly it is still UZ-LIBRARY-001 — the client's remedy is identical.
    try testing.expectEqualStrings(
        ec.ERR_LIBRARY_CURSOR_MALFORMED,
        codeForDecodeError(pagination.Error.VersionMismatch),
    );
}

test "test_library_cursor_codec_roundtrip: identity mismatch is UZ-LIBRARY-002, not 001" {
    // A cursor that decodes perfectly but belongs to another query. This is the
    // handler's rule, exercised through the same function the handler calls.
    const c = sample();

    // Same tenant, same limit — the only combination that may continue.
    try std.testing.expect(pagination.identityMatches(c.tenant_uuid, TENANT, c.limit, LIMIT));

    // Another tenant's cursor replayed against this principal. Nothing from the
    // cursor is trusted except the sort boundary, so this must be refused even
    // though the bytes are authentic.
    try std.testing.expect(!pagination.identityMatches(c.tenant_uuid, OTHER_TENANT, c.limit, LIMIT));

    // Same tenant, but the page size changed underneath the cursor. The boundary
    // was computed for a different window, so resuming from it would skip or
    // repeat rows depending on which direction the size moved.
    try std.testing.expect(!pagination.identityMatches(c.tenant_uuid, TENANT, c.limit, LIMIT + 1));
    try std.testing.expect(!pagination.identityMatches(c.tenant_uuid, TENANT, c.limit, 1));

    // And the two failure classes are genuinely different codes.
    try testing.expect(!std.mem.eql(u8, ec.ERR_LIBRARY_CURSOR_MALFORMED, ec.ERR_LIBRARY_CURSOR_MISMATCH));
    try testing.expectEqualStrings("UZ-LIBRARY-001", ec.ERR_LIBRARY_CURSOR_MALFORMED);
    try testing.expectEqualStrings("UZ-LIBRARY-002", ec.ERR_LIBRARY_CURSOR_MISMATCH);
}

/// A page size far above `MAX_LIMIT`, as an impatient client would send it.
/// Named for the same reason `http/pagination.zig` names it: the value is the
/// scenario, not an arbitrary number.
const WAY_OVER_MAX = "1000";

test "test_library_cursor_codec_roundtrip: limit bounds are UZ-LIBRARY-003 and never clamp" {
    // Out-of-range must ERROR. Clamping would serve a hundred rows to a caller
    // that asked for far more and let it believe it had the whole set — a bug
    // that only appears once a tenant outgrows one page.
    try testing.expectError(error.OutOfRange, pagination.parseLimit(WAY_OVER_MAX));
    try testing.expectError(error.OutOfRange, pagination.parseLimit("101"));
    try testing.expectError(error.OutOfRange, pagination.parseLimit("0"));
    try testing.expectError(error.OutOfRange, pagination.parseLimit("-1"));
    try testing.expectError(error.OutOfRange, pagination.parseLimit("abc"));

    // The inclusive range and the absent/empty defaults.
    try testing.expectEqual(@as(u32, 1), try pagination.parseLimit("1"));
    try testing.expectEqual(pagination.MAX_LIMIT, try pagination.parseLimit("100"));
    try testing.expectEqual(pagination.DEFAULT_LIMIT, try pagination.parseLimit(null));
    try testing.expectEqual(pagination.DEFAULT_LIMIT, try pagination.parseLimit(""));

    try testing.expectEqualStrings("UZ-LIBRARY-003", ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS);
}
