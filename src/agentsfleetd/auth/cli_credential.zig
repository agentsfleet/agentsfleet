//! Generation and shape-checking for `agentsfleet login` credentials — the
//! durable, user-scoped value that `core.cli_credentials` stores the digest of.
//!
//! Portability wall: std only, like the rest of `src/auth/`. Nothing here
//! reaches into `src/db/`; the store and the authenticate-path lookup live
//! elsewhere and consume what this module produces.
//!
//! Two properties are load-bearing and neither is decorative:
//!
//!   1. The raw value is drawn from the cryptographic random source at full
//!      entropy. `core.cli_credentials` stores a plain unsalted SHA-256, which
//!      cannot be inverted only while its input is unguessable. A generator
//!      that ever drew from a clock, a counter, or a non-cryptographic source
//!      would quietly turn that column into a reversible record of live
//!      credentials.
//!   2. The value is recognisable by shape. `looksWellFormed` is checked when a
//!      stored credential is LOADED, not merely when it is written, so a
//!      regression that persists a session token where a credential belongs is
//!      refused at read instead of travelling on a request. The reference
//!      implementation does the same (`~/Projects/oss/cli`,
//!      `internal/utils/access_token.go:16`, validated in `LoadAccessTokenFS`).

const std = @import("std");
const common = @import("common");

/// Prefix marking a value as an `agentsfleet` Command-Line Interface
/// credential. Present so a leaked value is identifiable in a log or a paste,
/// and so a differently-shaped value fails `looksWellFormed` rather than being
/// sent to a server that would merely reject it.
pub const PREFIX = "afc_";

/// 32 bytes of entropy. The digest that guards this value is unsalted, so the
/// input's unguessability is the whole defence; 256 bits is far beyond any
/// offline search and costs nothing to carry.
pub const RANDOM_BYTES: usize = 32;

/// Hex expansion of `RANDOM_BYTES`.
pub const BODY_LEN: usize = RANDOM_BYTES * 2;

/// Full rendered length: prefix plus hex body.
pub const TOTAL_LEN: usize = PREFIX.len + BODY_LEN;

/// Leading hex characters kept for display beside a credential in a list. Eight
/// of sixty-four leaves 224 bits unrevealed, so a displayed prefix narrows an
/// offline search by nothing that matters.
pub const DISPLAY_HEX_LEN: usize = 8;

/// Length of the rendered display prefix, e.g. `afc_a1b2c3d4`.
pub const DISPLAY_PREFIX_LEN: usize = PREFIX.len + DISPLAY_HEX_LEN;

/// Mint a fresh credential. The caller owns the returned bytes.
///
/// `common.secureRandomBytes` is the only source used, deliberately and
/// permanently — it is this project's single entropy surface (see
/// `src/lib/common/random.zig`), and property 1 in the module comment is why
/// no other source may be substituted here.
pub fn generate(alloc: std.mem.Allocator) ![]const u8 {
    var raw: [RANDOM_BYTES]u8 = undefined;
    try common.secureRandomBytes(&raw);

    const out = try alloc.alloc(u8, TOTAL_LEN);
    errdefer alloc.free(out);

    @memcpy(out[0..PREFIX.len], PREFIX);
    const hex = std.fmt.bytesToHex(raw, .lower);
    @memcpy(out[PREFIX.len..], &hex);
    return out;
}

/// The non-secret fragment stored alongside the digest so an operator can tell
/// two credentials apart in a list. Returns a slice of `credential`, so it
/// borrows rather than allocates; it is only valid while `credential` is.
pub fn displayPrefix(credential: []const u8) []const u8 {
    if (credential.len < DISPLAY_PREFIX_LEN) return credential;
    return credential[0..DISPLAY_PREFIX_LEN];
}

/// Whether `value` has this module's shape: exact length, exact prefix, and a
/// body of lower-case hex. Checked on LOAD as well as on write — a session
/// token, an empty string, or a truncated paste is refused here rather than
/// being sent to a server that would only answer `unauthorized`.
pub fn looksWellFormed(value: []const u8) bool {
    if (value.len != TOTAL_LEN) return false;
    if (!std.mem.startsWith(u8, value, PREFIX)) return false;
    for (value[PREFIX.len..]) |c| {
        const is_hex = (c >= '0' and c <= '9') or (c >= 'a' and c <= 'f');
        if (!is_hex) return false;
    }
    return true;
}

const testing = std.testing;

test "generated credential has the declared shape" {
    const cred = try generate(testing.allocator);
    defer testing.allocator.free(cred);

    try testing.expectEqual(TOTAL_LEN, cred.len);
    try testing.expect(std.mem.startsWith(u8, cred, PREFIX));
    try testing.expect(looksWellFormed(cred));
}

// Invariant 9. A generator that drew from a clock, a counter, or a
// non-cryptographic source would still satisfy the shape test above, so this
// asserts the property that actually guards the unsalted digest: across a large
// sample, every value is distinct and no body position is stuck.
test "credential is full entropy from a secure source" {
    const SAMPLE = 512;
    var seen = std.StringHashMap(void).init(testing.allocator);
    defer {
        var it = seen.keyIterator();
        while (it.next()) |k| testing.allocator.free(k.*);
        seen.deinit();
    }

    // A source with no entropy at a given position leaves that column constant
    // across every sample; tracking first-seen bytes catches it without
    // asserting any particular distribution.
    var varies = [_]bool{false} ** BODY_LEN;
    var first: [BODY_LEN]u8 = undefined;

    var i: usize = 0;
    while (i < SAMPLE) : (i += 1) {
        const cred = try generate(testing.allocator);
        const body = cred[PREFIX.len..];
        if (i == 0) {
            @memcpy(&first, body);
        } else {
            for (body, 0..) |c, pos| {
                if (c != first[pos]) varies[pos] = true;
            }
        }
        // Duplicate in 512 draws from 256 bits is impossible short of a broken
        // source, so the map owning `cred` doubles as the collision assertion.
        const gop = try seen.getOrPut(cred);
        if (gop.found_existing) {
            testing.allocator.free(cred);
            return error.DuplicateCredentialGenerated;
        }
    }

    try testing.expectEqual(@as(usize, SAMPLE), seen.count());
    for (varies) |v| try testing.expect(v);
}

test "looksWellFormed refuses everything that is not a credential" {
    try testing.expect(!looksWellFormed(""));
    try testing.expect(!looksWellFormed(PREFIX));
    // A session token — the exact regression this check exists to catch.
    try testing.expect(!looksWellFormed("eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.e30.x"));
    // Right length, wrong prefix.
    try testing.expect(!looksWellFormed("xxx_" ++ "a" ** BODY_LEN));
    // Right shape, upper-case body — the digest is over exact bytes, so a
    // case-folded value would hash differently and must not be accepted.
    try testing.expect(!looksWellFormed(PREFIX ++ "A" ** BODY_LEN));
    // Right length and prefix, non-hex body.
    try testing.expect(!looksWellFormed(PREFIX ++ "g" ** BODY_LEN));
    try testing.expect(looksWellFormed(PREFIX ++ "0" ** BODY_LEN));
}

test "displayPrefix reveals only the declared fragment" {
    const cred = try generate(testing.allocator);
    defer testing.allocator.free(cred);

    const shown = displayPrefix(cred);
    try testing.expectEqual(DISPLAY_PREFIX_LEN, shown.len);
    try testing.expect(std.mem.startsWith(u8, cred, shown));
    try testing.expect(shown.len < cred.len);
}
