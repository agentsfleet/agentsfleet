//! Canonical entity identifiers. Every `core.*` / `fleet.*` / `memory.*` row id
//! is a UUIDv7 minted here, so this file owns the one spelling the rest of the
//! system compares: lowercase hex, dashed, version 7, Request for Comments
//! (RFC) 4122 variant.
//!
//! Uppercase input is REJECTED, never normalized. Postgres folds `::uuid` to
//! lowercase, so an uppercase spelling would be the same row there but a
//! different key everywhere an id is handled as text — Redis dedupe keys
//! (`http/handlers/webhooks/github.zig`), session keys, `std.mem.eql`. One
//! entity with two valid spellings is the bug this rejection prevents.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;

/// Canonical dashed UUID text: 32 hex chars plus 4 dashes.
pub const UUID_TEXT_LEN: usize = 36;

/// Entropy UUIDv7 carries after the 48-bit timestamp. The version nibble and
/// variant bits overwrite 6 of these 80 bits, leaving 74 random.
const ENTROPY_LEN: usize = 10;

/// Bytes of the big-endian millisecond field, and the largest instant it can
/// hold (year 10889) — beyond it the high bits would silently vanish.
const TIMESTAMP_BYTES: usize = 6;
const MAX_TIMESTAMP_MS: u64 = 0xffff_ffff_ffff;

/// Raw-byte offsets of the two fields that make a UUID a *v7* UUID.
const VERSION_BYTE_INDEX: usize = 6;
const VARIANT_BYTE_INDEX: usize = 8;
const VERSION_7_HIGH_NIBBLE: u8 = 0x70;
const LOW_NIBBLE_MASK: u8 = 0x0f;
const VARIANT_RFC4122_HIGH_BITS: u8 = 0x80;
const VARIANT_CLEAR_MASK: u8 = 0x3f;

/// Text offsets of the same two fields, once dashes are in place.
const VERSION_CHAR_INDEX: usize = 14;
const VARIANT_CHAR_INDEX: usize = 19;
const VERSION_CHAR: u8 = '7';
const DASH_CHAR: u8 = '-';
const DASH_INDEXES = [_]usize{ 8, 13, 18, 23 };

pub fn generateWorkspaceId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateFleetId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateActivityEventId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateVaultSecretId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generatePlatformLlmKeyId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateRunnerId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateRunnerLeaseId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateRunnerEventId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateRunnerAffinityId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateFleetBundleId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateConnectorInstallId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateConnectorChannelId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateFleetLibraryId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateTenantModelEntryId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateUserPreferenceId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateScheduleId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn isSupportedFleetId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedTenantId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedWorkspaceId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isUuidV7(id: []const u8) bool {
    if (!isCanonicalUuid(id)) return false;
    if (id[VERSION_CHAR_INDEX] != VERSION_CHAR) return false;
    return switch (id[VARIANT_CHAR_INDEX]) {
        '8', '9', 'a', 'b' => true,
        else => false,
    };
}

/// Mint a UUIDv7. Returns the text by value, not a slice into a caller-supplied
/// buffer, so no id can outlive the storage it was written into.
pub fn generateUuidV7() ![UUID_TEXT_LEN]u8 {
    var entropy: [ENTROPY_LEN]u8 = undefined;
    try constants.secureRandomBytes(&entropy);
    return encodeUuidV7(clock.nowMillis(), entropy);
}

/// Mint a UUIDv7 on the heap. Caller must free.
pub fn allocUuidV7(alloc: std.mem.Allocator) ![]const u8 {
    const id = try generateUuidV7();
    return alloc.dupe(u8, &id);
}

/// Encode one UUIDv7 from an explicit instant and caller-supplied entropy.
///
/// Pure — reads neither the clock nor the entropy source — which is what lets
/// `id_format_test.zig` assert
/// the exact byte layout instead of only the shape. Production callers want
/// `generateUuidV7`.
pub fn encodeUuidV7(now_ms: i64, entropy: [ENTROPY_LEN]u8) ![UUID_TEXT_LEN]u8 {
    // A pre-epoch clock would wrap the cast into a far-future timestamp; a
    // post-year-10889 one would lose the bits the 48-bit field cannot hold.
    // Both are unrepresentable rather than merely unusual, so they fail loudly.
    if (now_ms < 0) return error.ClockBeforeUnixEpoch;
    const ts_ms: u64 = @intCast(now_ms);
    if (ts_ms > MAX_TIMESTAMP_MS) return error.TimestampOutOfRange;

    var raw: [TIMESTAMP_BYTES + ENTROPY_LEN]u8 = undefined;

    // Big-endian timestamp: byte order is what makes UUIDv7 text sort
    // chronologically, so the most significant byte has to land first.
    var timestamp_be: [@sizeOf(u64)]u8 = undefined;
    std.mem.writeInt(u64, &timestamp_be, ts_ms, .big);
    @memcpy(raw[0..TIMESTAMP_BYTES], timestamp_be[timestamp_be.len - TIMESTAMP_BYTES ..]);
    @memcpy(raw[TIMESTAMP_BYTES..], &entropy);

    raw[VERSION_BYTE_INDEX] = (raw[VERSION_BYTE_INDEX] & LOW_NIBBLE_MASK) | VERSION_7_HIGH_NIBBLE;
    raw[VARIANT_BYTE_INDEX] = (raw[VARIANT_BYTE_INDEX] & VARIANT_CLEAR_MASK) | VARIANT_RFC4122_HIGH_BITS;

    // Interleave the hex with dashes at DASH_INDEXES — the same constant
    // `isCanonicalUuid` checks, so the writer and the reader cannot drift on
    // where the groups break. Infallible by construction: the output width is
    // fixed, so there is no formatting error to swallow.
    const hex = std.fmt.bytesToHex(raw, .lower);
    var out: [UUID_TEXT_LEN]u8 = undefined;
    var hex_index: usize = 0;
    for (&out, 0..) |*c, text_index| {
        if (isDashIndex(text_index)) {
            c.* = DASH_CHAR;
        } else {
            c.* = hex[hex_index];
            hex_index += 1;
        }
    }
    return out;
}

fn isCanonicalUuid(id: []const u8) bool {
    if (id.len != UUID_TEXT_LEN) return false;
    for (DASH_INDEXES) |idx| {
        if (id[idx] != DASH_CHAR) return false;
    }
    for (id, 0..) |c, idx| {
        if (isDashIndex(idx)) continue;
        if (!isHexLower(c)) return false;
    }
    return true;
}

fn isDashIndex(idx: usize) bool {
    return std.mem.indexOfScalar(usize, &DASH_INDEXES, idx) != null;
}

/// Lowercase-only by design — see the module doc-comment on why an uppercase
/// alias is a correctness problem rather than a cosmetic one.
fn isHexLower(c: u8) bool {
    return (c >= '0' and c <= '9') or (c >= 'a' and c <= 'f');
}

test {
    _ = @import("id_format_test.zig");
}
