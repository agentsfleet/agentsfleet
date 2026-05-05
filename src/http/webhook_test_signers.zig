// Test-only signer that produces correctly signed GitHub webhook fixtures.
// Must NOT import from src/auth/middleware/* — signer and verifier share no code
// so a bug in one cannot silently cancel the other (RULE: signer/verifier isolation).

const std = @import("std");

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const Signed = struct {
    header_name: []const u8,
    header_value: []const u8, // heap-owned
    pub fn deinit(self: Signed, alloc: std.mem.Allocator) void {
        alloc.free(self.header_value);
    }
};

fn hmacHex(alloc: std.mem.Allocator, key: []const u8, msg: []const u8, prefix: []const u8) ![]u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    HmacSha256.create(&mac, msg, key);
    const hex = std.fmt.bytesToHex(mac, .lower);
    const out = try alloc.alloc(u8, prefix.len + hex.len);
    @memcpy(out[0..prefix.len], prefix);
    @memcpy(out[prefix.len..], &hex);
    return out;
}

// GitHub: `x-hub-signature-256: sha256=<hex>` over raw body bytes.
pub fn signGithub(alloc: std.mem.Allocator, secret: []const u8, body: []const u8) !Signed {
    return .{
        .header_name = "x-hub-signature-256",
        .header_value = try hmacHex(alloc, secret, body, "sha256="),
    };
}

test "signGithub produces 64-hex signature with sha256= prefix" {
    const alloc = std.testing.allocator;
    const s = try signGithub(alloc, "topsecret", "{\"event\":\"ping\"}");
    defer s.deinit(alloc);
    try std.testing.expectEqualStrings("x-hub-signature-256", s.header_name);
    try std.testing.expect(std.mem.startsWith(u8, s.header_value, "sha256="));
    try std.testing.expectEqual(@as(usize, "sha256=".len + 64), s.header_value.len);
}

test "signGithub deterministic — same inputs produce same output" {
    const alloc = std.testing.allocator;
    const a = try signGithub(alloc, "k", "body");
    defer a.deinit(alloc);
    const b = try signGithub(alloc, "k", "body");
    defer b.deinit(alloc);
    try std.testing.expectEqualStrings(a.header_value, b.header_value);
}

test "signGithub body mutation changes signature" {
    const alloc = std.testing.allocator;
    const a = try signGithub(alloc, "k", "body");
    defer a.deinit(alloc);
    const b = try signGithub(alloc, "k", "bodz");
    defer b.deinit(alloc);
    try std.testing.expect(!std.mem.eql(u8, a.header_value, b.header_value));
}
