//! Shared optimistic-concurrency capability: a strong ETag over an ordered
//! list of a resource's *editable* fields, plus the `If-Match` verdict.
//!
//! A handler opts in with three calls: `attach` the tag on the read (and on a
//! successful write), read `ifMatch` off the request, and ask `staleTag`
//! whether the caller's tag still names the current state. The mechanism lives
//! here once; the 412 *copy* stays per-resource (each adopter registers its
//! own error code) so the operator reads a sentence about their resource.
//!
//! Adopters:
//!   - the fleet source (`handlers/fleets/*`): hashes source + trigger markdown,
//!     so a lifecycle PATCH never 412s an editor with no source conflict.
//!   - the platform catalog row (`handlers/library/catalog_patch.zig`): hashes
//!     the operator-owned fields, so a stale re-send cannot discard the bundle.
//!
//! Declare a resource's editable surface as a fixed field list. Order and field
//! presence are part of the identity: each present value carries a byte-length
//! prefix, while null carries its own marker. Field boundaries, null, and the
//! empty string therefore remain distinct for every byte sequence.

const std = @import("std");

const Sha256 = std.crypto.hash.sha2.Sha256;

pub const HEADER_ETAG = "ETag";
/// httpz exposes request headers lowercased.
pub const HEADER_IF_MATCH = "if-match";
pub const HEADER_IF_NONE_MATCH = "if-none-match";

const FIELD_NULL = [_]u8{0};
const FIELD_PRESENT = [_]u8{1};

/// Quoted strong-ETag form per RFC 9110 (section 8.8.3): "<64 hex chars>".
/// `fields` is the resource's editable surface, in a fixed order. Present
/// fields contribute a marker, an eight-byte length, and their bytes; null
/// contributes a distinct marker. Caller owns the result.
pub fn compute(alloc: std.mem.Allocator, fields: []const ?[]const u8) ![]u8 {
    var hasher = Sha256.init(.{});
    for (fields) |field| {
        if (field) |f| {
            hasher.update(&FIELD_PRESENT);
            var len: [8]u8 = undefined;
            std.mem.writeInt(u64, &len, @intCast(f.len), .big);
            hasher.update(&len);
            hasher.update(f);
        } else {
            hasher.update(&FIELD_NULL);
        }
    }
    var digest: [Sha256.digest_length]u8 = undefined;
    hasher.final(&digest);
    const hex = std.fmt.bytesToHex(digest, .lower);
    return std.fmt.allocPrint(alloc, "\"{s}\"", .{hex});
}

/// The `If-Match` verdict. Returns the current tag when the caller sent an
/// `If-Match` that does NOT match (the 412 body carries it); null when the
/// caller matched or sent no `If-Match` at all (last-write-wins — the header
/// is opt-in). The returned tag is caller-owned.
pub fn staleTag(
    alloc: std.mem.Allocator,
    if_match: ?[]const u8,
    fields: []const ?[]const u8,
) !?[]u8 {
    const want = if_match orelse return null;
    const have = try compute(alloc, fields);
    if (matchesIfMatch(want, have)) {
        alloc.free(have);
        return null;
    }
    return have;
}

/// Strong comparison for the `If-Match` field-value grammar. The wildcard
/// matches any current representation; a comma-separated list matches when
/// any strong entity tag equals `have`. Weak tags never satisfy `If-Match`.
fn matchesIfMatch(raw: []const u8, have: []const u8) bool {
    const value = std.mem.trim(u8, raw, " \t");
    if (std.mem.eql(u8, value, "*")) return true;

    var rest = value;
    while (rest.len > 0) {
        rest = std.mem.trimStart(u8, rest, " \t");
        const weak = std.mem.startsWith(u8, rest, "W/");
        if (weak) rest = rest[2..];
        if (rest.len == 0 or rest[0] != '"') return false;
        const close = std.mem.indexOfScalarPos(u8, rest, 1, '"') orelse return false;
        const candidate = rest[0 .. close + 1];
        const matched = !weak and std.mem.eql(u8, candidate, have);
        rest = std.mem.trimStart(u8, rest[close + 1 ..], " \t");
        if (rest.len == 0) return matched;
        if (rest[0] != ',') return false;
        if (matched) return true;
        rest = rest[1..];
    }
    return false;
}

/// The `If-Match` request header, or null when the caller opted out.
pub fn ifMatch(req: anytype) ?[]const u8 {
    return req.header(HEADER_IF_MATCH);
}

/// The `If-None-Match` request header, or null when the caller sent none.
pub fn ifNoneMatch(req: anytype) ?[]const u8 {
    return req.header(HEADER_IF_NONE_MATCH);
}

/// Drop a leading weak-validator marker, if present.
fn stripWeak(tag: []const u8) []const u8 {
    return if (std.mem.startsWith(u8, tag, "W/")) tag[2..] else tag;
}

/// Whether a conditional GET may be answered 304.
///
/// `If-None-Match` uses the WEAK comparison function (RFC 9110 section 8.8.3.2),
/// which is the opposite of `If-Match` above and not an oversight in either
/// place: a write must not proceed against a representation that is only
/// semantically equivalent, while a revalidating cache that stored bytes under
/// `W/"x"` should still be told 304 when the current tag is `"x"` — both name
/// the same payload for the purpose of deciding whether to re-send it.
///
/// `*` matches any current representation: "send this only if I have nothing".
pub fn matchesIfNoneMatch(raw: []const u8, have: []const u8) bool {
    const value = std.mem.trim(u8, raw, " \t");
    if (std.mem.eql(u8, value, "*")) return true;
    const want = stripWeak(have);

    var rest = value;
    while (rest.len > 0) {
        const candidate_start = stripWeak(std.mem.trimStart(u8, rest, " \t"));
        if (candidate_start.len == 0 or candidate_start[0] != '"') return false;
        const close = std.mem.indexOfScalarPos(u8, candidate_start, 1, '"') orelse return false;
        if (std.mem.eql(u8, candidate_start[0 .. close + 1], want)) return true;

        rest = std.mem.trimStart(u8, candidate_start[close + 1 ..], " \t");
        if (rest.len == 0 or rest[0] != ',') return false;
        rest = rest[1..];
    }
    return false;
}

/// Set the `ETag` response header. Dupes into the response arena because the
/// header flushes after the handler returns, past the request arena's reuse.
pub fn attach(res: anytype, tag: []const u8) !void {
    try res.headerOpts(HEADER_ETAG, tag, .{ .dupe_value = true });
}

test "matchesIfNoneMatch: the wildcard matches any representation" {
    try std.testing.expect(matchesIfNoneMatch("*", "\"abc\""));
    try std.testing.expect(matchesIfNoneMatch("  *  ", "\"abc\""));
}

test "matchesIfNoneMatch: weak and strong forms both match" {
    // The whole difference from `If-Match`. A cache that stored the body under
    // the weak spelling must still revalidate to 304, or every conditional GET
    // it makes re-downloads a payload it already holds.
    try std.testing.expect(matchesIfNoneMatch("\"abc\"", "\"abc\""));
    try std.testing.expect(matchesIfNoneMatch("W/\"abc\"", "\"abc\""));
    try std.testing.expect(matchesIfNoneMatch("\"abc\"", "W/\"abc\""));
    try std.testing.expect(matchesIfNoneMatch("W/\"abc\"", "W/\"abc\""));
}

test "matchesIfNoneMatch: any member of a list matches" {
    try std.testing.expect(matchesIfNoneMatch("\"x\", \"abc\", \"y\"", "\"abc\""));
    try std.testing.expect(matchesIfNoneMatch("W/\"x\",W/\"abc\"", "\"abc\""));
    try std.testing.expect(!matchesIfNoneMatch("\"x\", \"y\"", "\"abc\""));
}

test "matchesIfNoneMatch: a non-match yields 200 rather than a bad 304" {
    // Answering 304 here would serve the caller a stale body it never asked to
    // keep — the one failure mode of a conditional read.
    try std.testing.expect(!matchesIfNoneMatch("\"stale\"", "\"abc\""));
    try std.testing.expect(!matchesIfNoneMatch("", "\"abc\""));
    try std.testing.expect(!matchesIfNoneMatch("garbage", "\"abc\""));
    try std.testing.expect(!matchesIfNoneMatch("\"unterminated", "\"abc\""));
    try std.testing.expect(!matchesIfNoneMatch("\"abc\" \"def\"", "\"def\""));
}

test "compute: quoted, hex, deterministic" {
    const a = try compute(std.testing.allocator, &.{ "skill", "trigger" });
    defer std.testing.allocator.free(a);
    const b = try compute(std.testing.allocator, &.{ "skill", "trigger" });
    defer std.testing.allocator.free(b);
    try std.testing.expectEqualStrings(a, b);
    try std.testing.expectEqual(@as(usize, Sha256.digest_length * 2 + 2), a.len);
    try std.testing.expectEqual(@as(u8, '"'), a[0]);
    try std.testing.expectEqual(@as(u8, '"'), a[a.len - 1]);
}

test "compute: field boundaries are unambiguous" {
    const ab_c = try compute(std.testing.allocator, &.{ "ab", "c" });
    defer std.testing.allocator.free(ab_c);
    const a_bc = try compute(std.testing.allocator, &.{ "a", "bc" });
    defer std.testing.allocator.free(a_bc);
    try std.testing.expect(!std.mem.eql(u8, ab_c, a_bc));
}

test "compute: null and empty are distinct" {
    const null_field = try compute(std.testing.allocator, &.{ "skill", null });
    defer std.testing.allocator.free(null_field);
    const empty_field = try compute(std.testing.allocator, &.{ "skill", "" });
    defer std.testing.allocator.free(empty_field);
    try std.testing.expect(!std.mem.eql(u8, null_field, empty_field));
}

test "compute: a null field differs from any content in that slot" {
    const with = try compute(std.testing.allocator, &.{ "skill", "t" });
    defer std.testing.allocator.free(with);
    const without = try compute(std.testing.allocator, &.{ "skill", null });
    defer std.testing.allocator.free(without);
    try std.testing.expect(!std.mem.eql(u8, with, without));
}

test "compute: field count is part of identity" {
    const two = try compute(std.testing.allocator, &.{ "a", "b" });
    defer std.testing.allocator.free(two);
    const one = try compute(std.testing.allocator, &.{"a"});
    defer std.testing.allocator.free(one);
    try std.testing.expect(!std.mem.eql(u8, two, one));
}

test "staleTag: no If-Match yields null (opt-in, last-write-wins)" {
    const verdict = try staleTag(std.testing.allocator, null, &.{"a"});
    try std.testing.expect(verdict == null);
}

test "staleTag: strong list and wildcard match; weak tag does not" {
    const alloc = std.testing.allocator;
    const current = try compute(alloc, &.{"a"});
    defer alloc.free(current);

    const list = try std.fmt.allocPrint(alloc, "\"other\", {s}", .{current});
    defer alloc.free(list);
    try std.testing.expect((try staleTag(alloc, list, &.{"a"})) == null);
    try std.testing.expect((try staleTag(alloc, "*", &.{"a"})) == null);

    const weak = try std.fmt.allocPrint(alloc, "W/{s}", .{current});
    defer alloc.free(weak);
    const stale = (try staleTag(alloc, weak, &.{"a"})) orelse return error.ExpectedStaleTag;
    defer alloc.free(stale);
    try std.testing.expectEqualStrings(current, stale);
}

test "staleTag: matching If-Match yields null" {
    const tag = try compute(std.testing.allocator, &.{ "name", "desc" });
    defer std.testing.allocator.free(tag);
    const verdict = try staleTag(std.testing.allocator, tag, &.{ "name", "desc" });
    try std.testing.expect(verdict == null);
}

test "staleTag: stale If-Match returns the current tag" {
    const current = try staleTag(std.testing.allocator, "\"stale\"", &.{ "name", "desc" });
    try std.testing.expect(current != null);
    defer std.testing.allocator.free(current.?);
    const expected = try compute(std.testing.allocator, &.{ "name", "desc" });
    defer std.testing.allocator.free(expected);
    try std.testing.expectEqualStrings(expected, current.?);
}
