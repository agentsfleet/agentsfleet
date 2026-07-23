const std = @import("std");
const id = @import("id_format.zig");

// ── Deterministic layout proofs ───────────────────────────────────────────
//
// These are the tests that actually pin UUIDv7 *construction*. Everything in
// the "shape" section below would still pass if the timestamp bytes were
// written in the wrong order, or if the entropy were silently dropped —
// `encodeUuidV7` takes the clock and the entropy as arguments precisely so the
// exact output can be asserted instead of only its shape.

// 0x0102_0304_0506 — every timestamp byte distinct, so a reversed or rotated
// write order cannot coincidentally produce the same text.
const KNOWN_TS_MS: i64 = 0x0102_0304_0506;
const KNOWN_ENTROPY = [10]u8{ 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa };
// pin test: literal is the contract
const KNOWN_UUID = "01020304-0506-7122-b344-5566778899aa";

test "encodeUuidV7 pins the exact byte layout for a known clock and entropy" {
    const out = try id.encodeUuidV7(KNOWN_TS_MS, KNOWN_ENTROPY);
    try std.testing.expectEqualStrings(KNOWN_UUID, &out);
}

test "timestamp occupies the leading 48 bits, most significant byte first" {
    const out = try id.encodeUuidV7(KNOWN_TS_MS, KNOWN_ENTROPY);
    // Bytes 0..5 render as text positions 0..7 and 9..12 (a dash at 8).
    try std.testing.expectEqualStrings("01020304", out[0..8]);
    try std.testing.expectEqualStrings("0506", out[9..13]);

    // Big-endian is the load-bearing property: a LATER instant must produce a
    // LEXICOGRAPHICALLY GREATER id, which is the only ordering UUIDv7 promises.
    const earlier = try id.encodeUuidV7(KNOWN_TS_MS, KNOWN_ENTROPY);
    const later = try id.encodeUuidV7(KNOWN_TS_MS + 1, KNOWN_ENTROPY);
    try std.testing.expect(std.mem.order(u8, &earlier, &later) == .lt);
}

test "version nibble and variant bits overwrite exactly their own bits" {
    const out = try id.encodeUuidV7(KNOWN_TS_MS, KNOWN_ENTROPY);
    // Version 7 replaces the HIGH nibble of entropy[0] (0x11 -> 0x71): the
    // low nibble '1' must survive.
    try std.testing.expectEqual(@as(u8, '7'), out[14]);
    try std.testing.expectEqual(@as(u8, '1'), out[15]);
    // Variant 10xx replaces the top TWO bits of entropy[2] (0x33 -> 0xb3):
    // 0x33 already has 0b00 up top, so the low six bits '3' must survive.
    try std.testing.expectEqual(@as(u8, 'b'), out[19]);
    try std.testing.expectEqual(@as(u8, '3'), out[20]);
}

test "every entropy byte the version and variant do not touch survives intact" {
    const out = try id.encodeUuidV7(KNOWN_TS_MS, KNOWN_ENTROPY);
    // entropy[1] and entropy[3..9] pass through untouched. Their absence would
    // mean the generator is discarding randomness it claims to carry.
    try std.testing.expectEqualStrings("22", out[16..18]);
    try std.testing.expectEqualStrings("44", out[21..23]);
    try std.testing.expectEqualStrings("5566778899aa", out[24..36]);
}

test "a distinct entropy vector produces a distinct id at the same instant" {
    var other = KNOWN_ENTROPY;
    other[9] ^= 0xff;
    const a = try id.encodeUuidV7(KNOWN_TS_MS, KNOWN_ENTROPY);
    const b = try id.encodeUuidV7(KNOWN_TS_MS, other);
    try std.testing.expect(!std.mem.eql(u8, &a, &b));
}

test "encodeUuidV7 output always passes its own validator" {
    const out = try id.encodeUuidV7(KNOWN_TS_MS, KNOWN_ENTROPY);
    try std.testing.expect(id.isUuidV7(&out));
}

// ── Encoding boundaries ───────────────────────────────────────────────────

test "a pre-epoch clock is rejected instead of wrapping into the far future" {
    try std.testing.expectError(error.ClockBeforeUnixEpoch, id.encodeUuidV7(-1, KNOWN_ENTROPY));
}

test "the most negative clock reading is rejected, not wrapped" {
    // The guard has to fire BEFORE the @intCast. minInt(i64) reinterpreted as
    // u64 is 0x8000_0000_0000_0000 — inside the 48-bit range check would be a
    // far-future id from a broken clock, and in a build with runtime safety off
    // the cast itself is illegal behaviour rather than a wrap.
    try std.testing.expectError(
        error.ClockBeforeUnixEpoch,
        id.encodeUuidV7(std.math.minInt(i64), KNOWN_ENTROPY),
    );
}

test "a timestamp wider than the 48-bit field is rejected, not truncated" {
    // pin test: literal is the contract — this IS the 48-bit ceiling under test
    const max_ms: i64 = 0xffff_ffff_ffff;
    // The last representable instant still encodes.
    _ = try id.encodeUuidV7(max_ms, KNOWN_ENTROPY);
    try std.testing.expectError(error.TimestampOutOfRange, id.encodeUuidV7(max_ms + 1, KNOWN_ENTROPY));
}

test "the epoch instant itself is representable" {
    const out = try id.encodeUuidV7(0, KNOWN_ENTROPY);
    try std.testing.expectEqualStrings("00000000", out[0..8]);
    try std.testing.expect(id.isUuidV7(&out));
}

// ── Canonical spelling: lowercase only ────────────────────────────────────

test "the canonical lowercase spelling is accepted" {
    try std.testing.expect(id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99"));
}

test "an uppercase spelling of a valid id is rejected, not normalized" {
    // Postgres folds these to one row on ::uuid; Redis dedupe keys, cache keys
    // and std.mem.eql see two. Accepting both spellings is the aliasing bug.
    try std.testing.expect(!id.isUuidV7("0195B4BA-8D3A-7F13-8ABC-2B3E1E0A6F99"));
    // Mixed case is the same defect, and the likelier accident.
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7F13-8abc-2b3e1e0a6f99"));
    // An uppercase variant nibble must not sneak in through the variant switch.
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-Babc-2b3e1e0a6f99"));
}

test "generated ids only ever use lowercase hex" {
    const out = try id.generateUuidV7();
    for (out) |c| {
        if (c == '-') continue;
        try std.testing.expect((c >= '0' and c <= '9') or (c >= 'a' and c <= 'f'));
    }
}

// ── Validator rejections ──────────────────────────────────────────────────

test "validators reject non-uuid inputs" {
    try std.testing.expect(!id.isSupportedWorkspaceId("not-a-uuid"));
    try std.testing.expect(!id.isSupportedTenantId("missing-uuid-shape"));
    try std.testing.expect(!id.isSupportedFleetId("0195b4ba8d3a7f138abc2b3e1e0a6f99"));
}

test "isUuidV7 rejects wrong length strings" {
    try std.testing.expect(!id.isUuidV7(""));
    try std.testing.expect(!id.isUuidV7("short"));
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f9")); // 35 chars
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f999")); // 37 chars
}

test "isUuidV7 rejects non-hex characters" {
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6gzz"));
    try std.testing.expect(!id.isUuidV7("zzzzzzzz-zzzz-7zzz-8zzz-zzzzzzzzzzzz"));
}

test "isUuidV7 rejects a misplaced dash" {
    try std.testing.expect(!id.isUuidV7("0195b4ba8-d3a-7f13-8abc-2b3e1e0a6f99"));
}

test "isUuidV7 rejects wrong version nibble" {
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-6f13-8abc-2b3e1e0a6f99"));
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-4f13-8abc-2b3e1e0a6f99"));
}

test "isUuidV7 rejects wrong variant nibble" {
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-0abc-2b3e1e0a6f99"));
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-cabc-2b3e1e0a6f99"));
}

test "isUuidV7 rejects the nil and max uuids" {
    try std.testing.expect(!id.isUuidV7("00000000-0000-0000-0000-000000000000"));
    try std.testing.expect(!id.isUuidV7("ffffffff-ffff-ffff-ffff-ffffffffffff"));
}

// ── Generators ────────────────────────────────────────────────────────────

const LIVE_GENERATORS = .{
    id.generateWorkspaceId,
    id.generateFleetId,
    id.generateActivityEventId,
    id.generateVaultSecretId,
    id.generatePlatformLlmKeyId,
    id.generateFleetBundleId,
    id.generateTenantModelEntryId,
    id.generateScheduleId,
};

test "all live id generators produce valid uuidv7 of the canonical length" {
    const alloc = std.testing.allocator;
    inline for (LIVE_GENERATORS) |gen| {
        const idd = try gen(alloc);
        defer alloc.free(idd);
        try std.testing.expect(id.isUuidV7(idd));
        try std.testing.expectEqual(id.UUID_TEXT_LEN, idd.len);
    }
}

test "generators produce ids that pass their own entity validators" {
    const alloc = std.testing.allocator;

    const workspace_id = try id.generateWorkspaceId(alloc);
    defer alloc.free(workspace_id);
    try std.testing.expect(id.isSupportedWorkspaceId(workspace_id));

    const fleet_id = try id.generateFleetId(alloc);
    defer alloc.free(fleet_id);
    try std.testing.expect(id.isSupportedFleetId(fleet_id));
}

test "generated ids are unique across calls" {
    const alloc = std.testing.allocator;
    const id1 = try id.generateFleetId(alloc);
    defer alloc.free(id1);
    const id2 = try id.generateFleetId(alloc);
    defer alloc.free(id2);
    try std.testing.expect(!std.mem.eql(u8, id1, id2));
}

test "ids from different generators are distinct" {
    const alloc = std.testing.allocator;
    const a = try id.generateWorkspaceId(alloc);
    defer alloc.free(a);
    const b = try id.generateFleetId(alloc);
    defer alloc.free(b);
    const c = try id.generateActivityEventId(alloc);
    defer alloc.free(c);
    try std.testing.expect(!std.mem.eql(u8, a, b));
    try std.testing.expect(!std.mem.eql(u8, b, c));
}

test "generator returns OutOfMemory when allocator fails" {
    var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    const result = id.generateFleetId(fa.allocator());
    try std.testing.expectError(error.OutOfMemory, result);
}

test "generateUuidV7 needs no allocator and returns an owned value" {
    // The by-value return is what removes the borrowed-buffer footgun: there is
    // no caller buffer whose scope could end before the id is used.
    const out = try id.generateUuidV7();
    try std.testing.expectEqual(id.UUID_TEXT_LEN, out.len);
    try std.testing.expect(id.isUuidV7(&out));
}

test "concurrent generation produces no duplicates" {
    const num_threads = 8;
    const ids_per_thread = 64;
    const total = num_threads * ids_per_thread;

    const Context = struct {
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        ids: [total][]const u8 = undefined,

        fn worker(self: *@This(), base: usize) void {
            const alloc = std.testing.allocator;
            for (0..ids_per_thread) |i| {
                self.ids[base + i] = id.generateFleetId(alloc) catch "FAILED";
            }
        }
    };
    var ctx: Context = .{};

    var threads: [num_threads]std.Thread = undefined;
    for (0..num_threads) |t| {
        threads[t] = try std.Thread.spawn(.{}, Context.worker, .{ &ctx, t * ids_per_thread });
    }
    for (&threads) |t| t.join();

    defer for (&ctx.ids) |idd| {
        if (!std.mem.eql(u8, idd, "FAILED")) std.testing.allocator.free(idd);
    };

    for (&ctx.ids) |idd| {
        try std.testing.expect(!std.mem.eql(u8, idd, "FAILED"));
        try std.testing.expect(id.isUuidV7(idd));
    }
    for (0..total) |i| {
        for (i + 1..total) |j| {
            try std.testing.expect(!std.mem.eql(u8, ctx.ids[i], ctx.ids[j]));
        }
    }
}

// Deliberately NOT asserted: that two ids minted in the same millisecond sort
// in generation order. This generator uses the plain-random construction of
// Request for Comments (RFC) 9562 — all 74 non-timestamp bits are random, with
// no counter — so within one millisecond the order is arbitrary, and a backward
// wall-clock step reorders across milliseconds too. Ordering queries pair
// `created_at` with the id as a tiebreaker (`state/fleet_events_store.zig`)
// rather than relying on id order, so nothing depends on the stronger promise.
