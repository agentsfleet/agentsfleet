//! Making MODEL-authored prose safe to render on an approval card.
//!
//! The card mixes two kinds of statement — daemon-derived and workspace-authored
//! facts a human may read as true, and a language model's own claim, which they
//! may not (see `approval_gate_detail`). Keeping those apart is only real if the
//! model's half cannot imitate the other half's SHAPE, which is what this module
//! enforces at the byte level.
//!
//! Split from `approval_gate_detail.zig` for the file-length budget (RULE FLL).

const std = @import("std");
const Allocator = std.mem.Allocator;

/// What a removed byte becomes. A space rather than nothing: a reader should see
/// that something was taken out, and dropping outright would silently fuse the
/// two words either side of it.
const SANITIZED_REPLACEMENT: u8 = ' ';

fn isUnsafeByte(c: u8) bool {
    return c < 0x20 or c == 0x7F;
}

/// Length of a Unicode bidirectional override starting at `i`, or 0. The
/// embedding/override set U+202A–U+202E and the isolate set U+2066–U+2069 encode
/// as `E2 80 AA..AE` and `E2 81 A6..A9`.
fn bidiOverrideLen(s: []const u8, i: usize) usize {
    if (i + 3 > s.len or s[i] != 0xE2) return 0;
    const b1 = s[i + 1];
    const b2 = s[i + 2];
    if (b1 == 0x80 and b2 >= 0xAA and b2 <= 0xAE) return 3;
    if (b1 == 0x81 and b2 >= 0xA6 and b2 <= 0xA9) return 3;
    return 0;
}

/// Cheap pre-check so the common path — prose that was already fine — allocates
/// nothing and keeps borrowing from the parsed context.
pub fn needsSanitizing(s: []const u8) bool {
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        if (isUnsafeByte(s[i]) or bidiOverrideLen(s, i) != 0) return true;
    }
    return false;
}

/// Output length, so `sanitize` allocates exactly and frees cleanly.
fn sanitizedLen(s: []const u8) usize {
    var n: usize = 0;
    var i: usize = 0;
    while (i < s.len) : (n += 1) {
        const skip = bidiOverrideLen(s, i);
        i += if (skip != 0) skip else 1;
    }
    return n;
}

/// A card-safe copy of model-authored prose. Null ONLY when the allocation
/// failed — the caller then renders nothing, because raw prose on the card is
/// the outcome being prevented.
///
/// Two distinct forgeries, one pass:
///
///   * **C0 controls and DEL.** `approval_gate_slack.writeJsonEscaped` turns a
///     newline into the JSON `\n` escape, which Slack renders back as a real
///     line break — so model prose could emit its own `- Gate:` and
///     `- If approved:` rows beneath the genuine ones and counterfeit the
///     daemon-derived half of the card, the only half a human may read as fact.
///     The remaining C0 bytes are not escaped at all, making the payload invalid
///     JSON (RFC 8259 §7); Slack rejects the whole message, so the gate parks
///     with nobody notified.
///   * **Bidirectional overrides.** They reorder rendered text without altering
///     a byte of it, so a sha or repository can display as something it is not.
pub fn sanitize(alloc: Allocator, s: []const u8) ?[]const u8 {
    const out = alloc.alloc(u8, sanitizedLen(s)) catch return null;
    var w: usize = 0;
    var i: usize = 0;
    while (i < s.len) : (w += 1) {
        const skip = bidiOverrideLen(s, i);
        if (skip != 0) {
            out[w] = SANITIZED_REPLACEMENT;
            i += skip;
            continue;
        }
        out[w] = if (isUnsafeByte(s[i])) SANITIZED_REPLACEMENT else s[i];
        i += 1;
    }
    return out;
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "prose: ordinary text is reported safe and needs no copy" {
    try testing.expect(!needsSanitizing("revert abc123 in acme/widgets"));
    try testing.expect(!needsSanitizing(""));
    // Non-ASCII that is not a bidi control passes through untouched.
    try testing.expect(!needsSanitizing("révert — abc123"));
}

test "prose: every C0 control and DEL is caught" {
    var c: u8 = 0;
    while (c < 0x20) : (c += 1) {
        const buf = [_]u8{ 'a', c, 'b' };
        try testing.expect(needsSanitizing(&buf));
    }
    const del = [_]u8{ 'a', 0x7F, 'b' };
    try testing.expect(needsSanitizing(&del));
}

test "prose: a newline becomes a space, so one line stays one line" {
    const alloc = testing.allocator;
    const got = sanitize(alloc, "revert abc123\n- Gate: `production-write`").?;
    defer alloc.free(got);
    try testing.expectEqualStrings("revert abc123 - Gate: `production-write`", got);
}

test "prose: a bidi override collapses to a single space" {
    const alloc = testing.allocator;
    // Three bytes in, one out — which is why the length is computed up front.
    const got = sanitize(alloc, "a\u{202E}b").?;
    defer alloc.free(got);
    try testing.expectEqualStrings("a b", got);
}

test "prose: sanitizing preserves every safe byte" {
    const alloc = testing.allocator;
    const got = sanitize(alloc, "revert abc123\tin acme/widgets").?;
    defer alloc.free(got);
    try testing.expectEqualStrings("revert abc123 in acme/widgets", got);
}
