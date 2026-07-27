//! Unit tests for the readiness index's pure half.
//!
//! `mark` / `clear` / `depth` are single Redis commands whose behaviour is only
//! meaningful against a live server, so they are proven in the integration tier.
//! What IS provable here is `decodePeek`, and it carries two assumptions worth
//! pinning:
//!
//!   1. **The wire shape.** `HRANDFIELD … WITHVALUES` returns a flat array
//!      alternating field and value under RESP2, and a nested array of pairs
//!      under RESP3. This client sends no `HELLO`, so RESP2 is the only shape it
//!      can see — but that is an assumption about a sibling file, and if someone
//!      adds a `HELLO 3` handshake these tests are what fails.
//!   2. **The allocation ladder.** Each entry duplicates two slices. A failure on
//!      the second must not leak the first, and a failure on entry N must not
//!      leak entries 0..N-1 — which Zig does not unwind for us.

const std = @import("std");
const fleet_ready = @import("fleet_ready.zig");
const redis_protocol = @import("redis_protocol.zig");

const testing = std.testing;

fn bulk(alloc: std.mem.Allocator, s: []const u8) !redis_protocol.RespValue {
    return .{ .bulk = try alloc.dupe(u8, s) };
}

/// A canonical UUIDv7 fleet id for pair `i` — the only field shape `decodePeek`
/// keeps now that it validates ids before binding them anywhere near the
/// candidate query's `uuid[]` cast.
fn fleetId(buf: *[36]u8, i: usize) ![]const u8 {
    return std.fmt.bufPrint(buf, "0195c9da-1e2a-7f13-8abc-{d:0>12}", .{i});
}

/// A RESP2 `HRANDFIELD … WITHVALUES` reply carrying `pairs` fleets, each with a
/// distinct token, in the flat `[f, v, f, v, …]` layout.
fn flatReply(alloc: std.mem.Allocator, pairs: usize) !redis_protocol.RespValue {
    const items = try alloc.alloc(redis_protocol.RespValue, pairs * 2);
    for (0..pairs) |i| {
        var id_buf: [36]u8 = undefined;
        var token_buf: [32]u8 = undefined;
        items[i * 2] = try bulk(alloc, try fleetId(&id_buf, i));
        items[i * 2 + 1] = try bulk(alloc, try std.fmt.bufPrint(&token_buf, "token-{d}", .{i}));
    }
    return .{ .array = items };
}

test "an empty reply decodes to no entries" {
    // Load-bearing: the lease path's zero-Postgres guarantee is the `len == 0`
    // early return. An empty hash must never yield entries to iterate.
    const empty_array = redis_protocol.RespValue{ .array = &.{} };
    const decoded = try fleet_ready.decodePeek(testing.allocator, empty_array);
    defer fleet_ready.freePeeked(testing.allocator, decoded);
    try testing.expectEqual(@as(usize, 0), decoded.len);

    const from_nil = try fleet_ready.decodePeek(testing.allocator, .{ .array = null });
    defer fleet_ready.freePeeked(testing.allocator, from_nil);
    try testing.expectEqual(@as(usize, 0), from_nil.len);
}

test "a non-array reply decodes to no entries rather than erroring" {
    // Redis answers a missing key with a nil/empty reply; treating an
    // unexpected scalar as "nothing ready" keeps a degraded datastore from
    // turning every poll into a logged error.
    const decoded = try fleet_ready.decodePeek(testing.allocator, .{ .integer = 0 });
    defer fleet_ready.freePeeked(testing.allocator, decoded);
    try testing.expectEqual(@as(usize, 0), decoded.len);
}

test "each field is paired with the value that follows it" {
    var reply = try flatReply(testing.allocator, 3);
    defer reply.deinit(testing.allocator);

    const entries = try fleet_ready.decodePeek(testing.allocator, reply);
    defer fleet_ready.freePeeked(testing.allocator, entries);
    try testing.expectEqual(@as(usize, 3), entries.len);
    // Pairing, not merely presence: a decoder that stepped by one, or that read
    // fields and values from separate halves of the array, would still produce
    // three entries but mis-associate every token.
    try testing.expectEqualStrings("0195c9da-1e2a-7f13-8abc-000000000000", entries[0].fleet_id);
    try testing.expectEqualStrings("token-0", entries[0].token);
    try testing.expectEqualStrings("0195c9da-1e2a-7f13-8abc-000000000002", entries[2].fleet_id);
    try testing.expectEqualStrings("token-2", entries[2].token);
}

test "a non-canonical field is skipped, never decoded into a candidate" {
    // One malformed field in the shared hash (an operator's stray HSET) bound
    // into the candidate query's uuid[] cast would error the WHOLE query on
    // every replica, presenting as a healthy-looking idle system. The decoder
    // therefore keeps only canonical UUIDv7 ids; `peek` heals the stray field
    // server-side (proven in the integration tier).
    const items = try testing.allocator.alloc(redis_protocol.RespValue, 4);
    items[0] = try bulk(testing.allocator, "not-a-uuid");
    items[1] = try bulk(testing.allocator, "token-junk");
    var id_buf: [36]u8 = undefined;
    items[2] = try bulk(testing.allocator, try fleetId(&id_buf, 7));
    items[3] = try bulk(testing.allocator, "token-7");
    var reply = redis_protocol.RespValue{ .array = items };
    defer reply.deinit(testing.allocator);

    const entries = try fleet_ready.decodePeek(testing.allocator, reply);
    defer fleet_ready.freePeeked(testing.allocator, entries);
    try testing.expectEqual(@as(usize, 1), entries.len);
    try testing.expectEqualStrings("0195c9da-1e2a-7f13-8abc-000000000007", entries[0].fleet_id);
    try testing.expectEqualStrings("token-7", entries[0].token);
}

test "the decoded entry count never exceeds the pairs the reply carried" {
    // The bound is enforced by the server (HRANDFIELD's count argument), so what
    // the decoder must not do is invent or drop entries.
    for ([_]usize{ 1, 2, 8, 64 }) |pairs| {
        var reply = try flatReply(testing.allocator, pairs);
        defer reply.deinit(testing.allocator);
        const decoded = try fleet_ready.decodePeek(testing.allocator, reply);
        defer fleet_ready.freePeeked(testing.allocator, decoded);
        try testing.expectEqual(pairs, decoded.len);
    }
}

test "an odd-length reply is rejected instead of decoding a truncated pair" {
    // An odd array means the reply is not the WITHVALUES shape at all — most
    // likely because a RESP3 handshake was added upstream. Erroring surfaces
    // that; reading `flat.len / 2` pairs would silently drop the last field and
    // keep working, which is the failure that hides.
    const items = try testing.allocator.alloc(redis_protocol.RespValue, 3);
    items[0] = try bulk(testing.allocator, "fleet-a");
    items[1] = try bulk(testing.allocator, "token-a");
    items[2] = try bulk(testing.allocator, "fleet-b");
    var reply = redis_protocol.RespValue{ .array = items };
    defer reply.deinit(testing.allocator);

    try testing.expectError(error.RedisUnexpectedResponse, fleet_ready.decodePeek(testing.allocator, reply));
}

test "a non-string field or value is rejected" {
    const items = try testing.allocator.alloc(redis_protocol.RespValue, 2);
    items[0] = try bulk(testing.allocator, "fleet-a");
    items[1] = .{ .integer = 7 };
    var reply = redis_protocol.RespValue{ .array = items };
    defer reply.deinit(testing.allocator);

    try testing.expectError(error.RedisUnexpectedResponse, fleet_ready.decodePeek(testing.allocator, reply));
}

fn decodeForLeakCheck(alloc: std.mem.Allocator, reply: *const redis_protocol.RespValue) !void {
    const decoded = try fleet_ready.decodePeek(alloc, reply.*);
    fleet_ready.freePeeked(alloc, decoded);
}

test "decodePeek frees every owned slice when any allocation fails" {
    // Fails each internal allocation in turn — the entries slice and both dupes
    // of every entry — and asserts the only error is OutOfMemory with zero
    // leaked bytes. This is what proves the outer errdefer frees exactly
    // `entries[0..filled]` and the inner one covers the half-built entry.
    var reply = try flatReply(testing.allocator, 4);
    defer reply.deinit(testing.allocator);
    try testing.checkAllAllocationFailures(testing.allocator, decodeForLeakCheck, .{&reply});
}

test "freeing an empty result is a no-op" {
    fleet_ready.freePeeked(testing.allocator, &.{});
}
