//! Case-insensitive ASCII string-keyed hashmap context, suitable for
//! `std.ArrayHashMapUnmanaged([]const u8, V, Context, true)`. Intended for
//! HTTP header maps (RFC 7230 §3.2 — header names compare case-insensitively).
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/bun.zig` — `CaseInsensitiveASCIIStringContext`, MIT). Adaptations:
//!   - `bun.copy` (memcpy with overlap-safety) → `@memcpy` (we never alias).
//!   - `bun.c.strncasecmp` (libc) → hand-rolled tolower-compare loop, so
//!     cross-compile to musl/aarch64 doesn't depend on libc string ops.
//!   - `Prehashed` value cache dropped — we don't have a hot path that
//!     re-hashes the same key shape repeatedly. Re-add when we do.
//!
//! Equality is ASCII-only: bytes outside A-Z / a-z compare bytewise. The
//! HTTP/1.1 header-name grammar is ASCII; non-ASCII bytes never appear in
//! conformant traffic, and folding them surprises the spec.

const std = @import("std");

const stack_buf_len = 1024;

/// Returns true when `a` and `b` are equal under ASCII fold of A-Z ↔ a-z.
/// Non-ASCII bytes compare bytewise.
pub fn eqlAsciiInsensitive(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    if (a.len == 0) return true;

    for (a, b) |ca, cb| {
        if (ca == cb) continue;
        const la: u8 = if (ca >= 'A' and ca <= 'Z') ca | 0x20 else ca;
        const lb: u8 = if (cb >= 'A' and cb <= 'Z') cb | 0x20 else cb;
        if (la != lb) return false;
    }
    return true;
}

/// Lowercase-fold `in` into `out`. `out.len >= in.len` required. Returns a
/// slice of `out` with length `in.len`.
fn copyLowerAscii(in: []const u8, out: []u8) []const u8 {
    std.debug.assert(out.len >= in.len);
    for (in, 0..) |c, i| {
        out[i] = if (c >= 'A' and c <= 'Z') c | 0x20 else c;
    }
    return out[0..in.len];
}

/// Wyhash of the lowercased form of `s`. Stack-buffers short strings (the
/// HTTP-header common case is well under 1024 bytes); chunks longer ones
/// through the streaming Wyhash API so the hash never depends on input
/// length-class.
fn hashLowerAscii(s: []const u8) u32 {
    var buf: [stack_buf_len]u8 = undefined;
    if (s.len <= stack_buf_len) {
        return @truncate(std.hash.Wyhash.hash(0, copyLowerAscii(s, &buf)));
    }
    var rest = s;
    var w = std.hash.Wyhash.init(0);
    while (rest.len > 0) {
        const n = @min(rest.len, stack_buf_len);
        w.update(copyLowerAscii(rest[0..n], &buf));
        rest = rest[n..];
    }
    return @truncate(w.final());
}

/// Context for `std.ArrayHashMapUnmanaged([]const u8, V, ..., true)`.
pub const CaseInsensitiveAsciiContext = struct {
    pub fn hash(_: @This(), s: []const u8) u32 {
        return hashLowerAscii(s);
    }
    pub fn eql(_: @This(), a: []const u8, b: []const u8, _: usize) bool {
        return eqlAsciiInsensitive(a, b);
    }
};

/// Type alias: `std.ArrayHashMapUnmanaged([]const u8, V, Context, true)`
/// keyed by case-insensitive ASCII strings. Caller manages key storage.
pub fn Map(comptime V: type) type {
    return std.ArrayHashMapUnmanaged([]const u8, V, CaseInsensitiveAsciiContext, true);
}

test "eql ascii-insensitive folds A-Z only" {
    try std.testing.expect(eqlAsciiInsensitive("Content-Type", "content-type"));
    try std.testing.expect(eqlAsciiInsensitive("X-Hub-Signature-256", "x-hub-signature-256"));
    try std.testing.expect(!eqlAsciiInsensitive("Content-Type", "Content-Length"));
    try std.testing.expect(!eqlAsciiInsensitive("foo", "fooo"));
    try std.testing.expect(eqlAsciiInsensitive("", ""));
    // Non-ASCII bytes compare bytewise — uppercase Ä (0xC4) does not fold to ä (0xE4).
    try std.testing.expect(!eqlAsciiInsensitive("\xC4", "\xE4"));
}

test "Map roundtrips case-insensitive lookup" {
    const alloc = std.testing.allocator;
    var m: Map([]const u8) = .empty;
    defer m.deinit(alloc);

    try m.put(alloc, "Authorization", "Bearer secret");
    try m.put(alloc, "Content-Type", "application/json");

    try std.testing.expectEqualStrings("Bearer secret", m.get("authorization").?);
    try std.testing.expectEqualStrings("Bearer secret", m.get("AUTHORIZATION").?);
    try std.testing.expectEqualStrings("application/json", m.get("content-type").?);
    try std.testing.expectEqual(@as(?[]const u8, null), m.get("X-Other"));
}

test "hash agrees on case-folded equivalents" {
    const ctx = CaseInsensitiveAsciiContext{};
    try std.testing.expectEqual(ctx.hash("X-Hub-Signature-256"), ctx.hash("x-hub-signature-256"));
    try std.testing.expectEqual(ctx.hash("a"), ctx.hash("A"));
    try std.testing.expectEqual(ctx.hash(""), ctx.hash(""));
}

test "hashLowerAscii handles strings longer than the stack buffer" {
    const len = stack_buf_len * 5 / 2; // 2560 bytes — forces the chunked path.
    var upper: [stack_buf_len * 3]u8 = undefined;
    var lower: [stack_buf_len * 3]u8 = undefined;
    for (upper[0..len], 0..) |*b, i| {
        const base: u8 = if ((i / 26) % 2 == 0) 'A' else 'a';
        b.* = base + @as(u8, @intCast(i % 26));
    }
    for (upper[0..len], 0..) |c, i| {
        lower[i] = if (c >= 'A' and c <= 'Z') c | 0x20 else c;
    }
    try std.testing.expectEqual(hashLowerAscii(upper[0..len]), hashLowerAscii(lower[0..len]));
}
